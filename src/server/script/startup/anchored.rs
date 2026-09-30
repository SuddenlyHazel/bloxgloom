//! Startup-only general anchored declarations; runtime callbacks stay server-side.
use super::*;
use crate::server::script::{anchored::ScriptAnchored, values::integer};
use bloxgloom_host_api::anchored::AnchoredBlockEntity;
#[path = "anchored/geometry.rs"]
mod geometry;

pub(super) fn declarer(
    lua: &Lua,
    pending: Rc<RefCell<Pending>>,
    namespace: &str,
    snapshot: Arc<PackageSnapshot>,
) -> mlua::Result<Function> {
    let namespace = namespace.to_owned();
    lua.create_function(move |_, value: Value| {
        let mut pending = pending.borrow_mut();
        let result = parse(value, &pending, &namespace, &snapshot);
        match result {
            Ok(declaration) => {
                pending.anchored.push(declaration);
                Ok(())
            }
            Err(error) => {
                pending.error.get_or_insert(error);
                Err(mlua::Error::RuntimeError(error.into()))
            }
        }
    })
}
fn parse(
    value: Value,
    pending: &Pending,
    namespace: &str,
    snapshot: &Arc<PackageSnapshot>,
) -> Result<AnchoredBlockEntity, &'static str> {
    if let Some(error) = pending.error {
        return Err(error);
    }
    if !snapshot.permits_anchored(namespace) {
        return Err("register_anchored requires content and anchored_entities/v1");
    }
    if pending.anchored.len() >= 8 {
        return Err("anchored declaration limit exceeded (8)");
    }
    let Value::Table(d) = value else {
        return Err("anchored declaration must be a table");
    };
    if d.metatable().is_some() {
        return Err("anchored declaration metatable forbidden");
    }
    for pair in d.clone().pairs::<Value, Value>().take(17) {
        let (Value::String(key), _) = pair.map_err(|_| "invalid anchored field")? else {
            return Err("invalid anchored field");
        };
        if !matches!(
            key.to_str().map_err(|_| "invalid anchored field")?.as_ref(),
            "entity"
                | "block"
                | "module"
                | "schema_version"
                | "max_state_bytes"
                | "max_public_bytes"
                | "interval"
                | "placement_item"
                | "anchor_state"
                | "placement_cost"
                | "removal_refund"
                | "footprint"
                | "observe"
                | "interaction"
        ) {
            return Err("unknown anchored field");
        }
    }
    let entity = text(field(&d, "entity")?)?;
    let block = text(field(&d, "block")?)?;
    for key in [&entity, &block] {
        owned(key, namespace)?;
    }
    if pending
        .anchored
        .iter()
        .any(|old| old.entity == entity || old.block == block)
        || pending
            .storage
            .iter()
            .any(|old| old.storage.entity == entity || old.storage.block == block)
        || pending
            .machines
            .iter()
            .any(|old| old.machine.entity == entity || old.machine.block == block)
        || pending.creatures.iter().any(|old| old.key == entity)
        || pending.entities.iter().any(|old| old.key == entity)
    {
        return Err("duplicate anchored identity or block");
    }
    let block_def = pending
        .blocks
        .iter()
        .find(|b| b.key == block)
        .ok_or("anchored needs a previously registered block")?;
    let anchor_state = state_key(field(&d, "anchor_state")?, placement_state(block_def))?;
    if !has_state(block_def, &anchor_state) {
        return Err("anchored anchor state must belong to the block");
    }
    let placement_item = optional_text(field(&d, "placement_item")?, block.clone())?;
    let (owner, local) = placement_item
        .split_once(':')
        .ok_or("invalid anchored placement item")?;
    if (owner != namespace && owner != "bloxgloom")
        || !super::super::package::manifest::identifier(local)
    {
        return Err("invalid anchored placement item");
    }
    let module = text(field(&d, "module")?)?;
    owned(&module, namespace)?;
    if snapshot.source(&module).is_none() {
        return Err("anchored module must be a declared server/shared package source");
    }
    let version = integer(field(&d, "schema_version")?, 1, u16::MAX.into())? as u16;
    let state_bytes = integer(field(&d, "max_state_bytes")?, 1, 65536)? as usize;
    let public_bytes = integer(field(&d, "max_public_bytes")?, 0, 4096)? as usize;
    let cost = optional_integer(field(&d, "placement_cost")?, 1, 128, 1)? as u16;
    let refund = optional_integer(
        field(&d, "removal_refund")?,
        0,
        i64::from(cost),
        i64::from(cost),
    )? as u16;
    let interval = integer(field(&d, "interval")?, 1, u32::MAX.into())? as u32;
    let footprint = geometry::footprint(field(&d, "footprint")?, &anchor_state, block_def)?;
    let observe = geometry::observe(field(&d, "observe")?)?;
    let interaction = match field(&d, "interaction")? {
        Value::Nil => b"activate".to_vec(),
        Value::String(bytes) if bytes.as_bytes().len() <= 239 => bytes.as_bytes().to_vec(),
        _ => return Err("anchored interaction must be at most 239 binary bytes"),
    };
    let declaration = AnchoredBlockEntity {
        entity,
        block,
        placement_item,
        anchor_state,
        footprint,
        placement_cost: cost,
        removal_refund: refund,
        schema_version: version,
        schema_fingerprint: snapshot.anchored_schema(&module, version),
        max_state_bytes: state_bytes,
        max_public_bytes: public_bytes,
        interval,
        observe,
        interaction,
        behavior: Arc::new(ScriptAnchored::server(
            Arc::clone(snapshot),
            module,
            state_bytes,
            public_bytes,
        )),
    };
    declaration
        .validate()
        .map_err(|_| "invalid anchored declaration")?;
    Ok(declaration)
}
fn owned(key: &str, namespace: &str) -> Result<(), &'static str> {
    if key.split_once(':').is_none_or(|(owner, local)| {
        owner != namespace || !super::super::package::manifest::identifier(local)
    }) {
        return Err("anchored keys/modules must belong to the package");
    }
    Ok(())
}
fn field(d: &mlua::Table, key: &str) -> Result<Value, &'static str> {
    d.raw_get(key).map_err(|_| "invalid anchored field")
}
fn optional_text(value: Value, default: String) -> Result<String, &'static str> {
    if value.is_nil() {
        Ok(default)
    } else {
        text(value)
    }
}
fn state_key(value: Value, default: String) -> Result<String, &'static str> {
    if value.is_nil() {
        return Ok(default);
    }
    let Value::String(value) = value else {
        return Err("anchored state key must be a string");
    };
    if value.as_bytes().is_empty() || value.as_bytes().len() > 1024 {
        return Err("anchored state key must contain 1..1024 UTF-8 bytes");
    }
    value
        .to_str()
        .map(|v| v.to_owned())
        .map_err(|_| "invalid anchored state key UTF-8")
}
fn optional_integer(value: Value, min: i64, max: i64, default: i64) -> Result<i64, &'static str> {
    if value.is_nil() {
        Ok(default)
    } else {
        integer(value, min, max)
    }
}
