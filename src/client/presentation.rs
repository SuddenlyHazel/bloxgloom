//! Local-only presentation protocol. One outstanding event per Session, no
//! retries/coalescing: busy input is rejected before changing local text. Fresh
//! VMs make state explicit; no imports, native handles, or gameplay commands.
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};

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
}

#[derive(Debug)]
pub(crate) enum Command {
    Text(String, String),
    Visible(String, bool),
    State(String),
}

#[derive(Debug)]
pub(crate) struct Reply {
    pub(crate) sequence: u32,
    pub(crate) result: Result<Vec<Command>, String>,
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
                    if sender
                        .send(Reply {
                            sequence,
                            result: run(request),
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
                "text" => {
                    Command::Text(text(&command, "node", 194)?, text(&command, "value", 128)?)
                }
                "visible" => {
                    let mlua::Value::Boolean(value) = command.raw_get("value")? else {
                        return Err(invalid());
                    };
                    Command::Visible(text(&command, "node", 194)?, value)
                }
                "state" => Command::State(text(&command, "value", 128)?),
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
