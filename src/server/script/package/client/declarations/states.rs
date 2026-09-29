//! Bounded V26 property/schema records and V28 per-state faces.
use super::*;

pub(super) fn encode(
    writer: &mut Writer,
    block: &content::Block,
    textures: bool,
) -> Result<(), ScriptError> {
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
        if (state.textures.is_some() && !textures) || state.properties.len() != properties.len() {
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
        if textures {
            writer.count(usize::from(state.textures.is_some()))?;
            if let Some(faces) = &state.textures {
                for key in [&faces.top, &faces.side, &faces.bottom] {
                    writer.field(key.as_bytes())?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    block: &mut content::Block,
    textures: bool,
    registered: &[content::Texture],
    owner: &str,
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
        let faces = if textures && reader.count(1)? == 1 {
            let mut face = || -> Result<String, ScriptError> {
                let key = reader.text(129)?;
                if key
                    .split_once(':')
                    .is_none_or(|(package, local)| package != owner || !identifier(local))
                    || !registered.iter().any(|texture| {
                        texture.key == key
                            && (block.material != Material::Cutout || texture.alpha_cutout)
                    })
                {
                    return Err(invalid());
                }
                Ok(key)
            };
            Some(content::FaceTextures {
                top: face()?,
                side: face()?,
                bottom: face()?,
            })
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
            textures: faces,
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
