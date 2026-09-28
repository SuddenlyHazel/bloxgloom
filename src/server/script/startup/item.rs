//! Bounded presentation options for non-placeable startup items.
use bloxgloom_host_api::content::DropSize;
use mlua::Value;

pub(super) fn options(options: Value) -> Result<(bool, DropSize), &'static str> {
    let table = match options {
        Value::Nil => return Ok((true, DropSize::Normal)),
        Value::Table(table) => table,
        _ => return Err("item options must be a table"),
    };
    let mut sprite = true;
    let mut size = DropSize::Normal;
    for pair in table.pairs::<Value, Value>().take(3) {
        let (key, value) = pair.map_err(|_| "invalid item option")?;
        let Value::String(key) = key else {
            return Err("invalid item option key");
        };
        match key.as_bytes().as_ref() {
            b"sprite" => {
                let Value::Boolean(value) = value else {
                    return Err("item sprite option must be boolean");
                };
                sprite = value;
            }
            b"drop_size" => {
                let Value::String(value) = value else {
                    return Err("item drop_size option must be small, normal or large");
                };
                size = match value.as_bytes().as_ref() {
                    b"small" => DropSize::Small,
                    b"normal" => DropSize::Normal,
                    b"large" => DropSize::Large,
                    _ => return Err("item drop_size option must be small, normal or large"),
                };
            }
            _ => return Err("unknown item option"),
        }
    }
    Ok((sprite, size))
}
