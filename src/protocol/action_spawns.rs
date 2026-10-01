//! Bounded committed mappings from attempt-local launch ordinals to exact IDs.
use super::*;
pub(crate) const MAX_ACTION_SPAWNS: usize = 32;
pub(crate) fn validate(values: &[SpawnReceipt]) -> io::Result<()> {
    if values.len() > MAX_ACTION_SPAWNS {
        return Err(invalid("too many action spawns"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        if usize::from(value.ordinal) != index
            || value.entity == 0
            || value.entity & (1u64 << 63) != 0
            || !seen.insert(value.entity)
        {
            return Err(invalid("invalid action spawn mapping"));
        }
    }
    Ok(())
}
pub(crate) fn write(out: &mut Vec<u8>, values: &[SpawnReceipt]) -> io::Result<()> {
    validate(values)?;
    out.push(values.len() as u8);
    for value in values {
        out.push(value.ordinal);
        out.extend(value.entity.to_le_bytes());
    }
    Ok(())
}
pub(super) fn read(cursor: &mut Cursor<'_>) -> io::Result<Vec<SpawnReceipt>> {
    let count = usize::from(cursor.u8()?);
    if count > MAX_ACTION_SPAWNS {
        return Err(invalid("too many action spawns"));
    }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        result.push(SpawnReceipt {
            ordinal: cursor.u8()?,
            entity: cursor.u64()?,
        });
    }
    validate(&result)?;
    Ok(result)
}

pub(crate) fn read_bytes(bytes: &[u8]) -> io::Result<(Vec<SpawnReceipt>, usize)> {
    let mut cursor = Cursor { bytes, offset: 0 };
    let result = read(&mut cursor)?;
    Ok((result, cursor.offset))
}
