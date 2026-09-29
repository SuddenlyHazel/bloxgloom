//! Bounded V26 property/schema records; no implicit combinations or VM values.
use super::*;

pub(super) fn encode(writer: &mut Writer, block: &content::Block) -> Result<(), ScriptError> {
    if block.properties.len() > 8 || block.states.is_empty() || block.states.len() > 32 {
        return Err(invalid());
    }
    let mut properties = block.properties.iter().collect::<Vec<_>>();
    properties.sort_by(|a, b| a.name.cmp(&b.name));
    writer.count(properties.len())?;
    for property in &properties {
        if property.name.len() > 32
            || !identifier(&property.name)
            || property.values.is_empty()
            || property.values.len() > 16
        {
            return Err(invalid());
        }
        writer.field(property.name.as_bytes())?;
        let mut values = property.values.clone();
        values.sort();
        if values.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(invalid());
        }
        writer.count(values.len())?;
        for value in values {
            if value.len() > 32 || !identifier(&value) {
                return Err(invalid());
            }
            writer.field(value.as_bytes())?;
        }
    }
    if properties
        .windows(2)
        .any(|pair| pair[0].name == pair[1].name)
    {
        return Err(invalid());
    }
    let mut states = block.states.iter().collect::<Vec<_>>();
    states.sort_by(|a, b| a.properties.cmp(&b.properties));
    writer.count(states.len())?;
    for state in states {
        if state.textures.is_some() || state.properties.len() != properties.len() {
            return Err(invalid());
        }
        for property in &properties {
            let value = state
                .properties
                .iter()
                .find(|(name, _)| name == &property.name)
                .ok_or_else(invalid)?;
            if !property.values.contains(&value.1) {
                return Err(invalid());
            }
            writer.field(value.1.as_bytes())?;
        }
        writer.count(usize::from(state.emission.is_some()))?;
        if let Some(emission) = state.emission {
            if emission > 15 {
                return Err(invalid());
            }
            writer.field(&[emission])?;
        }
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    block: &mut content::Block,
) -> Result<(), ScriptError> {
    let mut properties = Vec::new();
    for _ in 0..reader.count(8)? {
        let name = property_identifier(reader)?;
        if properties
            .last()
            .is_some_and(|last: &content::Property| last.name >= name)
        {
            return Err(invalid());
        }
        let mut values = Vec::new();
        for _ in 0..reader.count(16)? {
            let value = property_identifier(reader)?;
            if values.last().is_some_and(|last| last >= &value) {
                return Err(invalid());
            }
            values.push(value);
        }
        if values.is_empty() {
            return Err(invalid());
        }
        properties.push(content::Property { name, values });
    }
    let mut states = Vec::new();
    for _ in 0..reader.count(32)? {
        let mut values = Vec::new();
        for property in &properties {
            let value = property_identifier(reader)?;
            if !property.values.contains(&value) {
                return Err(invalid());
            }
            values.push((property.name.clone(), value));
        }
        let emission = if reader.count(1)? == 1 {
            let [emission] = reader.field(1)? else {
                return Err(invalid());
            };
            if *emission > 15 {
                return Err(invalid());
            }
            Some(*emission)
        } else {
            None
        };
        if states
            .last()
            .is_some_and(|last: &content::BlockState| last.properties >= values)
        {
            return Err(invalid());
        }
        states.push(content::BlockState {
            properties: values,
            textures: None,
            emission,
        });
    }
    if states.is_empty() {
        return Err(invalid());
    }
    block.properties = properties;
    block.states = states;
    Ok(())
}

fn property_identifier(reader: &mut Reader<'_>) -> Result<String, ScriptError> {
    let value = reader.identifier()?;
    if value.len() > 32 {
        return Err(invalid());
    }
    Ok(value)
}
