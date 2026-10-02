//! Startup command schemas: raw, dense, bounded declarations only.
use super::*;
use bloxgloom_host_api::actions::{
    CommandArgument, CommandPermission, FiniteNumber, MAX_COMMAND_ALIASES, MAX_COMMAND_ARGUMENTS,
    MAX_EXACT_INTEGER,
};

pub(super) fn declaration(values: mlua::Variadic<Value>) -> Result<Option<Command>, &'static str> {
    let value = match values.as_slice() {
        [] | [Value::Nil] => return Ok(None),
        [value] => value,
        _ => return Err("register_action expects at most one command descriptor"),
    };
    let table = plain(value.clone())?;
    let mut permission = None;
    let mut arguments = Vec::new();
    let mut aliases = Vec::new();
    for pair in table.pairs::<Value, Value>().take(4) {
        let (key, value) = pair.map_err(|_| "invalid command descriptor")?;
        match text(key)?.as_str() {
            "permission" => {
                permission = Some(match text(value)?.as_str() {
                    "Player" => CommandPermission::Player,
                    "Admin" => CommandPermission::Admin,
                    _ => return Err("command permission must be Player or Admin"),
                })
            }
            "arguments" => arguments = array(value, MAX_COMMAND_ARGUMENTS, argument)?,
            "aliases" => aliases = array(value, MAX_COMMAND_ALIASES, text)?,
            _ => return Err("unsupported command descriptor field"),
        }
    }
    let command = Command {
        permission: permission.ok_or("command requires explicit permission")?,
        arguments,
        aliases,
    };
    command
        .max_encoded_len()
        .ok_or("invalid bounded command schema")?;
    Ok(Some(command))
}
fn plain(value: Value) -> Result<mlua::Table, &'static str> {
    match value {
        Value::Table(table) if table.metatable().is_none() => Ok(table),
        _ => Err("command schema must be a plain table"),
    }
}
fn array<T>(
    value: Value,
    cap: usize,
    decode: impl Fn(Value) -> Result<T, &'static str>,
) -> Result<Vec<T>, &'static str> {
    let table = plain(value)?;
    let mut ordered: Vec<Option<T>> = (0..cap).map(|_| None).collect();
    let mut count = 0;
    for pair in table.pairs::<Value, Value>().take(cap + 1) {
        let (index, value) = pair.map_err(|_| "invalid command array")?;
        let index = integer(index, 1, cap as i64)? as usize - 1;
        ordered[index] = Some(decode(value)?);
        count += 1;
    }
    if count > cap || ordered[count..].iter().any(Option::is_some) {
        return Err("command arrays must be dense and bounded");
    }
    ordered
        .into_iter()
        .take(count)
        .map(|v| v.ok_or("command arrays must be dense"))
        .collect()
}
fn number(value: Value) -> Result<FiniteNumber, &'static str> {
    let value = match value {
        Value::Integer(n) => n as f64,
        Value::Number(n) => n,
        _ => return Err("command numeric bound must be a number"),
    };
    FiniteNumber::new(value).ok_or("command numeric bound must be finite")
}
fn argument(value: Value) -> Result<CommandArgument, &'static str> {
    let table = plain(value)?;
    let mut kind = None;
    let mut max_bytes = None;
    let mut default = None;
    let mut min = None;
    let mut max = None;
    for pair in table.pairs::<Value, Value>().take(6) {
        let (key, value) = pair.map_err(|_| "invalid command argument")?;
        match text(key)?.as_str() {
            "kind" => kind = Some(text(value)?),
            "max_bytes" => max_bytes = Some(integer(value, 1, 128)? as u8),
            "default" => default = Some(integer(value, 1, 128)? as u8),
            "min" => min = Some(value),
            "max" => max = Some(value),
            _ => return Err("unsupported command argument field"),
        }
    }
    let bounded_number = matches!(kind.as_deref(), Some("integer" | "number"));
    if bounded_number {
        if max_bytes.is_some() || default.is_some() {
            return Err("unsupported numeric argument fields");
        }
        let min = min.ok_or("numeric argument requires min")?;
        let max = max.ok_or("numeric argument requires max")?;
        return if kind.as_deref() == Some("integer") {
            Ok(CommandArgument::Integer {
                min: integer(min, -MAX_EXACT_INTEGER, MAX_EXACT_INTEGER)?,
                max: integer(max, -MAX_EXACT_INTEGER, MAX_EXACT_INTEGER)?,
            })
        } else {
            Ok(CommandArgument::Number {
                min: number(min)?,
                max: number(max)?,
            })
        };
    }
    if min.is_some() || max.is_some() {
        return Err("unexpected numeric argument bounds");
    }
    match kind.as_deref() {
        Some("player") if default.is_none() && max_bytes.is_none() => Ok(CommandArgument::Player),
        Some("item_key") if default.is_none() => Ok(CommandArgument::ItemKey {
            max_bytes: max_bytes.ok_or("key requires max_bytes")?,
        }),
        Some("entity_key") if default.is_none() => Ok(CommandArgument::EntityKey {
            max_bytes: max_bytes.ok_or("key requires max_bytes")?,
        }),
        Some("text") if default.is_none() => Ok(CommandArgument::Text {
            max_bytes: max_bytes.ok_or("text requires max_bytes")?,
        }),
        Some("count") if max_bytes.is_none() => Ok(CommandArgument::Count { default }),
        _ => Err("invalid command argument kind or fields"),
    }
}
#[cfg(test)]
mod tests;
