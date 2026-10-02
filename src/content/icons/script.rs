//! Shared bounded bitmap decoder for Luau startup and presentation replies.
use bloxgloom_host_api::icon::ItemIcon;
use mlua::{Table, Value};

pub(crate) fn decode(item: String, value: Value) -> mlua::Result<ItemIcon> {
    let Value::Table(table) = value else {
        return Err(invalid());
    };
    fields(&table, &["rows", "palette"])?;
    let rows: Table = table.raw_get("rows")?;
    let palette: Table = table.raw_get("palette")?;
    if rows.metatable().is_some()
        || palette.metatable().is_some()
        || rows.raw_len() == 0
        || rows.raw_len() > 32
    {
        return Err(invalid());
    }
    let mut parsed_rows = Vec::new();
    for pair in rows.clone().pairs::<Value, Value>().take(33) {
        let (index, value) = pair?;
        if !matches!(index, Value::Integer(n) if n >= 1 && n as usize <= rows.raw_len()) {
            return Err(invalid());
        }
        let Value::String(value) = value else {
            return Err(invalid());
        };
        if value.as_bytes().len() > 32 {
            return Err(invalid());
        }
        parsed_rows.push((index.as_integer().unwrap(), value.to_str()?.to_owned()));
    }
    if parsed_rows.len() != rows.raw_len() {
        return Err(invalid());
    }
    parsed_rows.sort_by_key(|(index, _)| *index);
    let mut colors = Vec::new();
    for pair in palette.pairs::<mlua::LuaString, Table>().take(33) {
        let (symbol, color) = pair?;
        if symbol.as_bytes().len() != 1 || color.raw_len() != 4 || color.metatable().is_some() {
            return Err(invalid());
        }
        let mut rgba = [0.0; 4];
        let mut count = 0;
        for pair in color.clone().pairs::<Value, Value>().take(5) {
            let (index, _) = pair?;
            if !matches!(index, Value::Integer(n) if (1..=4).contains(&n)) {
                return Err(invalid());
            }
            count += 1;
        }
        if count != 4 {
            return Err(invalid());
        }
        for (index, channel) in rgba.iter_mut().enumerate() {
            *channel = color.raw_get::<f32>(index + 1)?;
        }
        colors.push((symbol.as_bytes()[0], rgba));
    }
    colors.sort_by_key(|(symbol, _)| *symbol);
    let icon = ItemIcon {
        item,
        rows: parsed_rows.into_iter().map(|(_, row)| row).collect(),
        palette: colors,
    };
    icon.validate()
        .map_err(|error| mlua::Error::RuntimeError(error.0))?;
    Ok(icon)
}

pub(crate) fn fields(table: &Table, allowed: &[&str]) -> mlua::Result<()> {
    if table.metatable().is_some() {
        return Err(invalid());
    }
    for (index, pair) in table.clone().pairs::<String, Value>().enumerate() {
        let (key, _) = pair?;
        if index >= allowed.len() || !allowed.contains(&key.as_str()) {
            return Err(invalid());
        }
    }
    Ok(())
}
fn invalid() -> mlua::Error {
    mlua::Error::RuntimeError("invalid bounded item icon".into())
}

#[cfg(test)]
mod tests;
