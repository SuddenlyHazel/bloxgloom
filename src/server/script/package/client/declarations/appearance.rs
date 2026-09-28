//! V13's bounded, data-only appearance record. Identity includes exact f32 bits.
use super::*;
use bloxgloom_host_api::appearance::{Appearance, MAX_ADDITIONS};

pub(super) fn encode(writer: &mut Writer, appearance: &Appearance) -> Result<(), ScriptError> {
    appearance.validate().map_err(|_| invalid())?;
    writer.field(appearance.key.as_bytes())?;
    writer.field(&appearance.revision.to_le_bytes())?;
    writer.field(appearance.model.as_bytes())?;
    for palette in &appearance.palettes {
        writer.count(palette.len())?;
        for color in palette {
            let bytes: Vec<u8> = color
                .iter()
                .flat_map(|component| component.to_le_bytes())
                .collect();
            writer.field(&bytes)?;
        }
    }
    Ok(())
}

pub(super) fn decode(
    reader: &mut Reader<'_>,
    packages: &[composition::Package],
) -> Result<Appearance, ScriptError> {
    let key = reader.text(129)?;
    let (owner, local) = key.split_once(':').ok_or_else(invalid)?;
    if !identifier(local)
        || !packages.iter().any(|package| {
            package.key == format!("{owner}:package")
                && package.requires.iter().any(|r| r == composition::CONTENT)
        })
    {
        return Err(invalid());
    }
    let revision = u32::from_le_bytes(reader.field(4)?.try_into().map_err(|_| invalid())?);
    let model = reader.text(64)?;
    let mut palettes = std::array::from_fn(|_| Vec::new());
    for palette in &mut palettes {
        for _ in 0..reader.count(MAX_ADDITIONS)? {
            let bytes = reader.field(12)?;
            if bytes.len() != 12 {
                return Err(invalid());
            }
            palette.push(std::array::from_fn(|index| {
                f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
            }));
        }
    }
    let result = Appearance {
        key,
        revision,
        model,
        palettes,
    };
    result.validate().map_err(|_| invalid())?;
    Ok(result)
}
