//! Small explicit property/state vocabulary; never allocate an implicit product.
use super::*;
use bloxgloom_host_api::content::Property;
use mlua::Table;

pub(super) fn properties(value: Value) -> Result<Vec<Property>, &'static str> {
    let Value::Table(table) = value else {
        return Err("properties must be a table");
    };
    if table.metatable().is_some() {
        return Err("property metatables are unsupported");
    }
    let mut result = Vec::new();
    for pair in table.pairs::<Value, Value>().take(9) {
        if result.len() == 8 {
            return Err("too many block properties");
        }
        let (name, values) = pair.map_err(|_| "invalid property")?;
        let name = identifier(text(name)?)?;
        let Value::Table(values) = values else {
            return Err("property values must be an array");
        };
        let values = sequence(values, 16)?
            .into_iter()
            .map(|value| identifier(text(value)?))
            .collect::<Result<Vec<_>, _>>()?;
        if values.is_empty() {
            return Err("property values cannot be empty");
        }
        let mut unique = values.clone();
        unique.sort();
        unique.dedup();
        if unique.len() != values.len() {
            return Err("duplicate property value");
        }
        result.push(Property { name, values });
    }
    if result.is_empty() {
        return Err("properties cannot be empty");
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    if result.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err("duplicate block property");
    }
    Ok(result)
}

pub(super) fn states(
    value: Value,
    properties: &[Property],
) -> Result<Vec<BlockState>, &'static str> {
    let Value::Table(table) = value else {
        return Err("states must be an array");
    };
    let entries = sequence(table, 32)?;
    if entries.is_empty() {
        return Err("states cannot be empty");
    }
    let mut result = Vec::new();
    for value in entries {
        let Value::Table(table) = value else {
            return Err("state must be a table");
        };
        if table.metatable().is_some() {
            return Err("state metatables are unsupported");
        }
        let mut state = BlockState::default();
        for (index, pair) in table.pairs::<Value, Value>().enumerate() {
            if index >= 9 {
                return Err("too many state fields");
            }
            let (name, value) = pair.map_err(|_| "invalid state field")?;
            let name = text(name)?;
            if name == "emission" {
                state.emission = Some(integer(value, 0, 15)? as u8);
            } else {
                let property = properties
                    .iter()
                    .find(|property| property.name == name)
                    .ok_or("unknown state property")?;
                let value = identifier(text(value)?)?;
                if !property.values.contains(&value) {
                    return Err("state value not in property schema");
                }
                state.properties.push((name, value));
            }
        }
        if state.properties.len() != properties.len() {
            return Err("state must specify every property");
        }
        state.properties.sort();
        if state
            .properties
            .windows(2)
            .any(|pair| pair[0].0 == pair[1].0)
        {
            return Err("duplicate state property");
        }
        result.push(state);
    }
    result.sort_by(|a, b| a.properties.cmp(&b.properties));
    if result
        .windows(2)
        .any(|pair| pair[0].properties == pair[1].properties)
    {
        return Err("duplicate block state");
    }
    Ok(result)
}

fn sequence(table: Table, maximum: usize) -> Result<Vec<Value>, &'static str> {
    if table.metatable().is_some() || table.raw_len() > maximum {
        return Err("invalid bounded array");
    }
    let mut entries = Vec::new();
    for pair in table.pairs::<Value, Value>().take(maximum + 1) {
        if entries.len() == maximum {
            return Err("array limit exceeded");
        }
        let (index, value) = pair.map_err(|_| "invalid array member")?;
        let Value::Integer(index) = index else {
            return Err("array indices must be integers");
        };
        if index < 1 || index as usize > maximum {
            return Err("invalid array index");
        }
        entries.push((index as usize, value));
    }
    entries.sort_by_key(|(index, _)| *index);
    if entries
        .iter()
        .enumerate()
        .any(|(position, (index, _))| position + 1 != *index)
    {
        return Err("array must be dense");
    }
    Ok(entries.into_iter().map(|(_, value)| value).collect())
}

fn identifier(value: String) -> Result<String, &'static str> {
    if value.len() > 32 || !super::super::super::package::manifest::identifier(&value) {
        return Err("invalid property identifier");
    }
    Ok(value)
}
