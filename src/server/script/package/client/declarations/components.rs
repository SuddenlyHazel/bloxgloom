//! V27 opaque item schema identity; bytes belong to stacks, never the bundle.
use super::*;

pub(super) fn encode(writer: &mut Writer, schema: &content::Components) -> Result<(), ScriptError> {
    match schema {
        content::Components::None => writer.field(&[0])?,
        content::Components::Opaque {
            version,
            fingerprint,
            max_bytes,
            required,
        } => {
            if *version == 0 || *fingerprint == 0 || !(1..=1024).contains(max_bytes) {
                return Err(invalid());
            }
            writer.field(&[1])?;
            writer.field(&version.to_le_bytes())?;
            writer.field(&fingerprint.to_le_bytes())?;
            writer.field(&max_bytes.to_le_bytes())?;
            writer.field(&[u8::from(*required)])?;
        }
        content::Components::Unstructured => return Err(invalid()),
    }
    Ok(())
}

pub(super) fn decode(reader: &mut Reader<'_>) -> Result<content::Components, ScriptError> {
    match reader.field(1)? {
        [0] => Ok(content::Components::None),
        [1] => {
            let version = u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
            let fingerprint =
                u64::from_le_bytes(reader.field(8)?.try_into().map_err(|_| invalid())?);
            let max_bytes = u16::from_le_bytes(reader.field(2)?.try_into().map_err(|_| invalid())?);
            let [required] = reader.field(1)? else {
                return Err(invalid());
            };
            if version == 0 || fingerprint == 0 || !(1..=1024).contains(&max_bytes) || *required > 1
            {
                return Err(invalid());
            }
            Ok(content::Components::Opaque {
                version,
                fingerprint,
                max_bytes,
                required: *required == 1,
            })
        }
        _ => Err(invalid()),
    }
}
