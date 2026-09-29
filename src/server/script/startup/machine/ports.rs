//! Bounded Luau automation-port declarations. Slot indices are one-based in
//! source and converted once to the host's zero-based inventory slots.
use super::*;

pub(super) fn parse(value: Value, slots: u8) -> Result<Vec<api::Port>, &'static str> {
    let Value::Table(list) = value else {
        return if value.is_nil() {
            Ok(Vec::new())
        } else {
            Err("machine ports must be a list")
        };
    };
    let count = sequence_len(&list, 8)?;
    let mut result = Vec::with_capacity(count);
    for index in 1..=count {
        let port: mlua::Table = list.raw_get(index).map_err(|_| "invalid machine port")?;
        if port.metatable().is_some() {
            return Err("invalid machine port");
        }
        let name = text(field(&port, "name")?)?;
        if name.len() > 64 || result.iter().any(|old: &api::Port| old.name == name) {
            return Err("invalid or duplicate machine port name");
        }
        let Value::Table(faces) = field(&port, "faces")? else {
            return Err("machine port faces must be a list");
        };
        let face_count = sequence_len(&faces, 6)?;
        if face_count == 0 {
            return Err("machine port requires a face");
        }
        let mut normals = Vec::with_capacity(face_count);
        for face_index in 1..=face_count {
            let face: mlua::Table = faces
                .raw_get(face_index)
                .map_err(|_| "invalid machine face")?;
            if sequence_len(&face, 3)? != 3 {
                return Err("machine face needs three axes");
            }
            let mut normal = [0; 3];
            for (axis, coordinate) in normal.iter_mut().enumerate() {
                *coordinate = integer(
                    face.raw_get(axis + 1).map_err(|_| "invalid machine face")?,
                    -1,
                    1,
                )? as i32;
            }
            if !api::FACES.contains(&normal) || normals.contains(&normal) {
                return Err("invalid or duplicate machine face");
            }
            normals.push(normal);
        }
        let insert = slot_list(field(&port, "insert")?, slots)?;
        let extract = slot_list(field(&port, "extract")?, slots)?;
        if insert.is_empty() && extract.is_empty() {
            return Err("machine port needs insert or extract slots");
        }
        result.push(api::Port {
            name,
            faces: normals,
            insert,
            extract,
        });
    }
    Ok(result)
}

fn slot_list(value: Value, slots: u8) -> Result<Vec<u8>, &'static str> {
    let Value::Table(list) = value else {
        return if value.is_nil() {
            Ok(Vec::new())
        } else {
            Err("machine port slots must be a list")
        };
    };
    let count = sequence_len(&list, usize::from(slots))?;
    let mut result = Vec::with_capacity(count);
    for index in 1..=count {
        let slot = integer(
            list.raw_get(index)
                .map_err(|_| "invalid machine port slot")?,
            1,
            i64::from(slots),
        )? as u8
            - 1;
        if result.contains(&slot) {
            return Err("duplicate machine port slot");
        }
        result.push(slot);
    }
    Ok(result)
}

pub(super) fn sequence_len(table: &mlua::Table, maximum: usize) -> Result<usize, &'static str> {
    if table.metatable().is_some() {
        return Err("machine list cannot have a metatable");
    }
    let count = table.raw_len();
    if count > maximum
        || table
            .clone()
            .pairs::<Value, Value>()
            .take(maximum + 1)
            .count()
            != count
    {
        return Err("machine list is sparse or too long");
    }
    Ok(count)
}
