//! Exact owner IDs at the Luau boundary; no u64/u128 passes through a float.
use super::*;

pub(super) fn partition(value: Value) -> Result<api::Partition, &'static str> {
    match value {
        Value::Nil => Ok(api::Partition::Chunk),
        Value::String(value) if value.as_bytes().as_ref() == b"chunk" => Ok(api::Partition::Chunk),
        Value::String(value) if value.as_bytes().as_ref() == b"entity" => {
            Ok(api::Partition::Entity)
        }
        Value::String(value) if value.as_bytes().as_ref() == b"profile" => {
            Ok(api::Partition::Profile)
        }
        _ => Err("system partition must be chunk, entity or profile"),
    }
}

pub(super) fn seed(partition: api::Partition, value: &Table) -> Result<api::Owner, &'static str> {
    match partition {
        api::Partition::Chunk => Ok(api::Owner::Chunk(cell(
            field(value, "x")?,
            field(value, "y")?,
            field(value, "z")?,
        )?)),
        api::Partition::Entity => {
            let id = field(value, "id")?;
            if let Value::String(value) = &id {
                let id = hex_id(value, 16)? as u64;
                if id == 0 {
                    return Err("entity ID must be nonzero");
                }
                return Ok(api::Owner::Entity(id));
            }
            let words = words(id, 2)?;
            Ok(api::Owner::Entity(entity(words[0], words[1])))
        }
        api::Partition::Profile => {
            let id = field(value, "id")?;
            if let Value::String(value) = &id {
                return Ok(api::Owner::Profile(hex_id(value, 32)?));
            }
            let words = words(id, 4)?;
            Ok(api::Owner::Profile(profile([
                words[0], words[1], words[2], words[3],
            ])))
        }
    }
}

pub(super) fn word(value: Value) -> Result<u32, &'static str> {
    integer(value, 0, u32::MAX.into()).map(|value| value as u32)
}

pub(super) fn entity(lo: u32, hi: u32) -> u64 {
    u64::from(lo) | (u64::from(hi) << 32)
}

pub(super) fn profile(words: [u32; 4]) -> u128 {
    u128::from(words[0])
        | (u128::from(words[1]) << 32)
        | (u128::from(words[2]) << 64)
        | (u128::from(words[3]) << 96)
}

fn words(value: Value, count: usize) -> Result<Vec<u32>, &'static str> {
    let Value::Table(table) = value else {
        return Err("owner ID must be an exact word list");
    };
    if table.metatable().is_some()
        || table.raw_len() != count
        || table
            .clone()
            .pairs::<Value, Value>()
            .take(count + 1)
            .count()
            != count
    {
        return Err("owner ID must be a dense exact word list");
    }
    (1..=count)
        .map(|index| {
            integer(
                table.raw_get(index).map_err(|_| "invalid owner ID word")?,
                0,
                u32::MAX.into(),
            )
            .map(|value| value as u32)
        })
        .collect()
}

pub(super) fn present(lua: &Lua, owner: api::Owner) -> mlua::Result<(&'static str, Value)> {
    Ok(match owner {
        api::Owner::Chunk(cell) => {
            let value = lua.create_sequence_from(cell)?;
            value.set_readonly(true);
            ("chunk", Value::Table(value))
        }
        api::Owner::Entity(id) => (
            "entity",
            Value::UserData(crate::server::script::handles::entity(lua, id)?),
        ),
        api::Owner::Profile(id) => (
            "profile",
            Value::UserData(crate::server::script::handles::profile(lua, id)?),
        ),
    })
}

fn hex_id(value: &mlua::LuaString, digits: usize) -> Result<u128, &'static str> {
    let bytes = value.as_bytes();
    if bytes.len() != digits || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err("owner ID must be a fixed-width hexadecimal string");
    }
    u128::from_str_radix(value.to_str().map_err(|_| "invalid owner ID")?.as_ref(), 16)
        .map_err(|_| "invalid owner ID")
}
