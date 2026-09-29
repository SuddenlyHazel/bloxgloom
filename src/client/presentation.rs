//! Bounded presentation commands. A script may name its own registered semantic
//! action, but the client composes the request and the server owns its effects.
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};
mod visual;
pub(crate) use visual::VisualSession;
mod effects;
pub(crate) use effects::EffectBuffer;
#[cfg(test)]
#[path = "presentation/tests.rs"]
mod tests;

#[derive(Debug)]
pub(crate) struct Script {
    pub(crate) module: String,
    pub(crate) source: String,
}

pub(crate) struct Request {
    pub(crate) script: Arc<Script>,
    pub(crate) sequence: u32,
    pub(crate) event: String,
    pub(crate) value: String,
    pub(crate) state: String,
    pub(crate) texts: Vec<(String, String)>,
    pub(crate) replica: bool,
    pub(crate) entities: Vec<EntityView>,
    /// IDs entering or leaving the bounded presented window since the prior
    /// dispatched entity callback. Leaving does not imply server despawn.
    pub(crate) entered: Vec<u64>,
    pub(crate) left: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct EntityView {
    pub(crate) id: u64,
    pub(crate) key: String,
    pub(crate) position: [f32; 3],
    pub(crate) revision: u64,
    pub(crate) motion_revision: u64,
    /// Exact public codec bytes from the installed authoritative replica.
    pub(crate) public: Vec<u8>,
}

pub(crate) fn window_changes(previous: &[u64], current: &[EntityView]) -> (Vec<u64>, Vec<u64>) {
    let current_ids = current.iter().map(|entity| entity.id).collect::<Vec<_>>();
    let entered = current_ids
        .iter()
        .copied()
        .filter(|id| previous.binary_search(id).is_err())
        .collect();
    let left = previous
        .iter()
        .copied()
        .filter(|id| current_ids.binary_search(id).is_err())
        .collect();
    (entered, left)
}

#[derive(Debug)]
pub(crate) enum Command {
    Text(String, String),
    Visible(String, bool),
    State(String),
    Action(String, Vec<u8>),
    /// A client-only offset applied after authoritative pose reconstruction.
    Visual(u64, [f32; 3]),
    /// Client-only RGB multiplier for an offered entity's rendered model.
    Tint(u64, [f32; 3]),
    /// Bounded client-only ember, attached to an offered entity.
    Ember(u64, [f32; 3]),
    /// Short-lived colored spark attached to an offered entity.
    Spark(u64, [f32; 3], [f32; 3], f32, u16),
}

#[derive(Debug)]
pub(crate) struct Reply {
    pub(crate) sequence: u32,
    pub(crate) result: Result<Vec<Command>, String>,
    pub(crate) replica: bool,
    pub(crate) offered_entities: Vec<u64>,
    pub(crate) entity_batch: bool,
    pub(crate) anchor_batch: bool,
    pub(crate) offered_anchor_positions: Vec<(u64, [f32; 3])>,
}

#[derive(Debug)]
pub(crate) struct Worker {
    pub(crate) requests: SyncSender<Request>,
    pub(crate) replies: Receiver<Reply>,
}

impl Worker {
    pub(crate) fn spawn() -> std::io::Result<Self> {
        let (requests, receiver) = mpsc::sync_channel::<Request>(1);
        let (sender, replies) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("client-presentation".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let sequence = request.sequence;
                    let replica = request.replica;
                    let offered_entities =
                        request.entities.iter().map(|entity| entity.id).collect();
                    let entity_batch = request.event == "replica:entities";
                    let anchor_batch = request.event == "replica:anchors";
                    let offered_anchor_positions = if anchor_batch {
                        request
                            .entities
                            .iter()
                            .map(|entity| (entity.id, entity.position))
                            .collect()
                    } else {
                        vec![]
                    };
                    if sender
                        .send(Reply {
                            sequence,
                            result: run(request),
                            replica,
                            offered_entities,
                            entity_batch,
                            anchor_batch,
                            offered_anchor_positions,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        // Drop closes both channels without joining/blocking the window thread.
        // At most one admitted bounded invocation can still finish after drop.
        Ok(Self { requests, replies })
    }
}

fn run(request: Request) -> Result<Vec<Command>, String> {
    let module = crate::server::SourceModule {
        id: request.script.module.clone(),
        source: request.script.source.clone(),
    };
    crate::server::run_presentation(module, |lua, entry| {
        let input = lua.create_table()?;
        input.raw_set("sequence", request.sequence)?;
        input.raw_set("event", request.event.as_str())?;
        input.raw_set("value", request.value)?;
        input.raw_set("state", request.state)?;
        let texts = lua.create_table()?;
        for (id, text) in request.texts {
            texts.raw_set(id, text)?;
        }
        input.raw_set("texts", texts)?;
        let entities = lua.create_table()?;
        for (index, entity) in request.entities.iter().enumerate() {
            let view = lua.create_table()?;
            view.raw_set("id_lo", entity.id as u32)?;
            view.raw_set("id_hi", (entity.id >> 32) as u32)?;
            view.raw_set("key", entity.key.as_str())?;
            view.raw_set("revision_lo", entity.revision as u32)?;
            view.raw_set("revision_hi", (entity.revision >> 32) as u32)?;
            view.raw_set("motion_revision_lo", entity.motion_revision as u32)?;
            view.raw_set("motion_revision_hi", (entity.motion_revision >> 32) as u32)?;
            view.raw_set("public", lua.create_string(&entity.public)?)?;
            let position = lua.create_sequence_from(entity.position)?;
            position.set_readonly(true);
            view.raw_set("position", position)?;
            view.set_readonly(true);
            entities.raw_set(index + 1, view)?;
        }
        entities.set_readonly(true);
        input.raw_set("entities", entities)?;
        for (name, ids) in [("entered", request.entered), ("left", request.left)] {
            let list = lua.create_table_with_capacity(ids.len(), 0)?;
            for (index, id) in ids.into_iter().enumerate() {
                let value = lua.create_table()?;
                value.raw_set("id_lo", id as u32)?;
                value.raw_set("id_hi", (id >> 32) as u32)?;
                value.set_readonly(true);
                list.raw_set(index + 1, value)?;
            }
            list.set_readonly(true);
            input.raw_set(name, list)?;
        }
        let output: mlua::Table = entry.call(input)?;
        // Do not trust raw_len alone: sparse/associative tables must not bypass
        // traversal limits. Reject the seventeenth pair before decoding it.
        let count = output.raw_len();
        if count > 16 {
            return Err(invalid());
        }
        let mut seen = 0;
        for pair in output.clone().pairs::<mlua::Value, mlua::Value>().take(17) {
            let (key, _) = pair?;
            seen += 1;
            if !matches!(key, mlua::Value::Integer(i) if i > 0 && i as usize <= count) || seen > 16
            {
                return Err(invalid());
            }
        }
        if seen != count {
            return Err(invalid());
        }
        let mut commands = Vec::with_capacity(count);
        for index in 1..=count {
            let command: mlua::Table = output.raw_get(index)?;
            let op = text(&command, "op", 16)?;
            commands.push(match op.as_str() {
                "text" => Command::Text(
                    text(&command, "node", 194)?,
                    display_text(&command, "value", 128)?,
                ),
                "visible" => {
                    let mlua::Value::Boolean(value) = command.raw_get("value")? else {
                        return Err(invalid());
                    };
                    Command::Visible(text(&command, "node", 194)?, value)
                }
                "state" => Command::State(display_text(&command, "value", 128)?),
                "action" => {
                    let arguments = match command.raw_get::<mlua::Value>("arguments")? {
                        mlua::Value::Nil => Vec::new(),
                        mlua::Value::String(value)
                            if value.as_bytes().len()
                                <= bloxgloom_host_api::actions::MAX_REQUEST_ARGUMENTS =>
                        {
                            value.as_bytes().to_vec()
                        }
                        _ => return Err(invalid()),
                    };
                    Command::Action(text(&command, "key", 129)?, arguments)
                }
                "visual" if request.replica => {
                    let lo = word(&command, "id_lo")?;
                    let hi = word(&command, "id_hi")?;
                    let id = u64::from(lo) | (u64::from(hi) << 32);
                    let pose = [
                        bounded_float(
                            &command,
                            "yaw",
                            -std::f32::consts::PI,
                            std::f32::consts::PI,
                        )?,
                        bounded_float(&command, "bob", -0.25, 0.25)?,
                        bounded_float(&command, "squash", -0.25, 0.25)?,
                    ];
                    Command::Visual(id, pose)
                }
                "tint" if request.replica => {
                    let lo = word(&command, "id_lo")?;
                    let hi = word(&command, "id_hi")?;
                    let id = u64::from(lo) | (u64::from(hi) << 32);
                    let tint = [
                        bounded_float(&command, "r", 0.0, 1.0)?,
                        bounded_float(&command, "g", 0.0, 1.0)?,
                        bounded_float(&command, "b", 0.0, 1.0)?,
                    ];
                    Command::Tint(id, tint)
                }
                "ember" if request.replica => {
                    let lo = word(&command, "id_lo")?;
                    let hi = word(&command, "id_hi")?;
                    let id = u64::from(lo) | (u64::from(hi) << 32);
                    let offset = [
                        bounded_float(&command, "x", -1.0, 1.0)?,
                        bounded_float(&command, "y", -1.0, 1.0)?,
                        bounded_float(&command, "z", -1.0, 1.0)?,
                    ];
                    Command::Ember(id, offset)
                }
                "spark" if request.replica => {
                    let lo = word(&command, "id_lo")?;
                    let hi = word(&command, "id_hi")?;
                    let id = u64::from(lo) | (u64::from(hi) << 32);
                    let offset = [
                        bounded_float(&command, "x", -1.0, 1.0)?,
                        bounded_float(&command, "y", -1.0, 1.0)?,
                        bounded_float(&command, "z", -1.0, 1.0)?,
                    ];
                    let color = [
                        bounded_float(&command, "r", 0.0, 1.0)?,
                        bounded_float(&command, "g", 0.0, 1.0)?,
                        bounded_float(&command, "b", 0.0, 1.0)?,
                    ];
                    let size = optional_bounded_float(&command, "size", 0.05, 0.5, 0.16)?;
                    let lifetime_ms = match command.raw_get("lifetime_ms")? {
                        mlua::Value::Nil => 850,
                        mlua::Value::Integer(value) if (100..=2000).contains(&value) => {
                            value as u16
                        }
                        _ => return Err(invalid()),
                    };
                    Command::Spark(id, offset, color, size, lifetime_ms)
                }
                _ => return Err(invalid()),
            });
        }
        Ok(commands)
    })
    .map_err(|error| {
        // Module identity is host-owned; script error strings are untrusted.
        format!(
            "{} event {} #{}: {}",
            request.script.module,
            request.event,
            request.sequence,
            error.to_string().chars().take(512).collect::<String>()
        )
    })
}

fn invalid() -> mlua::Error {
    mlua::Error::RuntimeError("invalid local-ui command result".into())
}

fn text(table: &mlua::Table, key: &str, max: usize) -> mlua::Result<String> {
    let mlua::Value::String(value) = table.raw_get(key)? else {
        return Err(invalid());
    };
    let bytes = value.as_bytes();
    if bytes.len() > max || !bytes.iter().all(|b| (32..=126).contains(b)) {
        return Err(invalid());
    }
    Ok(value.to_str()?.to_owned())
}

fn display_text(table: &mlua::Table, key: &str, max: usize) -> mlua::Result<String> {
    let mlua::Value::String(value) = table.raw_get(key)? else {
        return Err(invalid());
    };
    let text = value.to_str()?;
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(invalid());
    }
    Ok(text.to_owned())
}

fn word(table: &mlua::Table, key: &str) -> mlua::Result<u32> {
    let mlua::Value::Integer(value) = table.raw_get(key)? else {
        return Err(invalid());
    };
    u32::try_from(value).map_err(|_| invalid())
}

fn bounded_float(table: &mlua::Table, key: &str, min: f32, max: f32) -> mlua::Result<f32> {
    let value = match table.raw_get(key)? {
        mlua::Value::Integer(value) => value as f64,
        mlua::Value::Number(value) => value,
        _ => return Err(invalid()),
    };
    if !value.is_finite() || value < f64::from(min) || value > f64::from(max) {
        return Err(invalid());
    }
    Ok(value as f32)
}

fn optional_bounded_float(
    table: &mlua::Table,
    key: &str,
    min: f32,
    max: f32,
    default: f32,
) -> mlua::Result<f32> {
    if matches!(table.raw_get::<mlua::Value>(key)?, mlua::Value::Nil) {
        Ok(default)
    } else {
        bounded_float(table, key, min, max)
    }
}
