//! Bounded worker-side decoding of dynamic widget declarations and control values.
use super::invalid;
use mlua::{Lua, Value};

#[derive(Clone, Debug)]
pub(crate) enum ControlValue {
    Text(String),
    Bool(bool),
    Number(f64),
}
impl ControlValue {
    pub(super) fn lua(self, lua: &Lua) -> mlua::Result<Value> {
        Ok(match self {
            Self::Text(text) => Value::String(lua.create_string(text)?),
            Self::Bool(value) => Value::Boolean(value),
            Self::Number(value) => Value::Number(value),
        })
    }
}

pub(super) fn display_text(input: Value) -> mlua::Result<String> {
    if !matches!(input, Value::String(_)) {
        return Err(invalid());
    }
    value(input)
}

pub(super) fn value(value: Value) -> mlua::Result<String> {
    let text = match value {
        Value::String(value) => value.to_str()?.to_owned(),
        Value::Boolean(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) if value.is_finite() => {
            let text = value.to_string();
            if text.len() > 128 {
                format!("{value:e}")
            } else {
                text
            }
        }
        _ => return Err(invalid()),
    };
    if text.len() > 1024 || text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(invalid());
    }
    Ok(text)
}

fn sequence(value: Value, max: usize) -> mlua::Result<Vec<mlua::Table>> {
    let Value::Table(table) = value else {
        return Err(invalid());
    };
    if table.metatable().is_some() || table.raw_len() > max {
        return Err(invalid());
    }
    let length = table.raw_len();
    let mut seen = 0;
    for pair in table.clone().pairs::<Value, Value>().take(max + 1) {
        let (key, _) = pair?;
        seen += 1;
        if !matches!(key,Value::Integer(n) if n>0 && n as usize<=length) || seen > max {
            return Err(invalid());
        }
    }
    if seen != length {
        return Err(invalid());
    }
    (1..=length).map(|i| table.raw_get(i)).collect()
}

fn record(
    table: mlua::Table,
    options: bool,
    budget: &mut usize,
) -> mlua::Result<serde_json::Value> {
    if table.metatable().is_some() {
        return Err(invalid());
    }
    let mut object = serde_json::Map::new();
    for pair in table.pairs::<Value, Value>().take(17) {
        let (Value::String(key), value) = pair? else {
            return Err(invalid());
        };
        let key = key.to_str()?.to_owned();
        let allowed = if options {
            matches!(key.as_str(), "key" | "label")
        } else {
            matches!(
                key.as_str(),
                "id" | "parent"
                    | "kind"
                    | "style"
                    | "text"
                    | "image"
                    | "event"
                    | "checked"
                    | "value"
                    | "min"
                    | "max"
                    | "step"
                    | "options"
                    | "selected"
            )
        };
        if !allowed {
            return Err(invalid());
        }
        let converted = match value {
            Value::String(value) => {
                let text = value.to_str()?.to_owned();
                if text.len() > 1024 {
                    return Err(invalid());
                }
                *budget = budget.checked_add(text.len()).ok_or_else(invalid)?;
                serde_json::Value::String(text)
            }
            Value::Boolean(value) => serde_json::Value::Bool(value),
            Value::Integer(value) => serde_json::Value::Number(value.into()),
            Value::Number(value) => {
                serde_json::Value::Number(serde_json::Number::from_f64(value).ok_or_else(invalid)?)
            }
            value if key == "options" && !options => serde_json::Value::Array(
                sequence(value, 64)?
                    .into_iter()
                    .map(|t| record(t, true, budget))
                    .collect::<mlua::Result<_>>()?,
            ),
            _ => return Err(invalid()),
        };
        if *budget > 32 * 1024 {
            return Err(invalid());
        }
        object.insert(key, converted);
    }
    Ok(serde_json::Value::Object(object))
}

pub(super) fn nodes(value: Value) -> mlua::Result<Vec<crate::ui::authored::RawNode>> {
    let mut budget = 0;
    sequence(value, 256)?
        .into_iter()
        .map(|table| {
            serde_json::from_value(record(table, false, &mut budget)?).map_err(|_| invalid())
        })
        .collect()
}

#[cfg(test)]
mod tests;
