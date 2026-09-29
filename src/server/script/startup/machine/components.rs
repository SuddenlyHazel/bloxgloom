//! Component predicates and exact outputs are immutable registration constants.
use super::*;
use bloxgloom_host_api::machine::{ComponentMatch, ComponentOutput, ComponentValue};

pub(super) fn input(value: Value) -> Result<ComponentMatch, &'static str> {
    match value {
        Value::Nil => Ok(ComponentMatch::Empty),
        Value::String(text) if text.as_bytes().as_ref() == b"empty" => Ok(ComponentMatch::Empty),
        Value::String(text) if text.as_bytes().as_ref() == b"present" => {
            Ok(ComponentMatch::Present)
        }
        Value::Table(table) => Ok(ComponentMatch::Exact(exact(table)?)),
        _ => Err("invalid machine input components"),
    }
}

pub(super) fn output(value: Value) -> Result<ComponentOutput, &'static str> {
    match value {
        Value::Nil => Ok(ComponentOutput::Empty),
        Value::String(text) if text.as_bytes().as_ref() == b"empty" => Ok(ComponentOutput::Empty),
        Value::String(text) if text.as_bytes().as_ref() == b"preserve_input" => {
            Ok(ComponentOutput::PreserveInput)
        }
        Value::Table(table) => Ok(ComponentOutput::Exact(exact(table)?)),
        _ => Err("invalid machine output components"),
    }
}

fn exact(table: mlua::Table) -> Result<ComponentValue, &'static str> {
    if table.metatable().is_some() {
        return Err("machine exact components cannot have a metatable");
    }
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair.map_err(|_| "invalid machine component field")?;
        if !matches!(key, Value::String(ref key) if key.as_bytes().as_ref() == b"version" || key.as_bytes().as_ref() == b"bytes")
        {
            return Err("unknown machine component field");
        }
    }
    let version = integer(field(&table, "version")?, 1, u16::MAX.into())? as u16;
    let Value::String(bytes) = field(&table, "bytes")? else {
        return Err("machine component bytes must be binary string");
    };
    if bytes.as_bytes().is_empty() || bytes.as_bytes().len() > 1024 {
        return Err("machine component bytes exceed bound");
    }
    Ok(ComponentValue {
        version,
        bytes: bytes.as_bytes().to_vec(),
    })
}
