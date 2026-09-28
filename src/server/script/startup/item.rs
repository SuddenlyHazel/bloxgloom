//! One bounded presentation option for non-placeable startup items.
use mlua::Value;

pub(super) fn sprite(options: Value) -> Result<bool, &'static str> {
    let table = match options {
        Value::Nil => return Ok(true),
        Value::Table(table) => table,
        _ => return Err("item options must be a table"),
    };
    let mut sprite = true;
    for pair in table.pairs::<Value, Value>().take(2) {
        let (key, value) = pair.map_err(|_| "invalid item option")?;
        let Value::String(key) = key else {
            return Err("invalid item option key");
        };
        if key.as_bytes().as_ref() != b"sprite" {
            return Err("unknown item option");
        }
        let Value::Boolean(value) = value else {
            return Err("item sprite option must be boolean");
        };
        sprite = value;
    }
    Ok(sprite)
}
