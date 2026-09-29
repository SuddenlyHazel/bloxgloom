//! Luau machine work proposals. The host still resolves ports, peer inventories
//! and exact stacks; these records contain no item creation or mutation power.
use super::*;
use crate::server::script::values::text;

pub(super) fn parse(
    value: Value,
    ports: &[String],
    own_slots: usize,
) -> Result<Vec<api::Work>, &'static str> {
    let Value::Table(list) = value else {
        return match value {
            Value::Boolean(true) => Ok(vec![api::Work::Process]),
            Value::Boolean(false) => Ok(Vec::new()),
            _ => Err("machine work must be a boolean or list"),
        };
    };
    if list.metatable().is_some() || list.raw_len() > 8 {
        return Err("machine work list exceeds eight entries");
    }
    let count = list.raw_len();
    if list.clone().pairs::<Value, Value>().take(9).count() != count {
        return Err("machine work list must be dense");
    }
    let mut result = Vec::with_capacity(count);
    for index in 1..=count {
        let work: mlua::Table = list.raw_get(index).map_err(|_| "invalid machine work")?;
        if work.metatable().is_some() {
            return Err("machine work cannot have a metatable");
        }
        let kind = text(
            work.raw_get("kind")
                .map_err(|_| "invalid machine work kind")?,
        )?;
        if kind == "process" {
            result.push(api::Work::Process);
        } else if kind == "transfer" {
            result.push(transfer(&work, ports, own_slots)?);
        } else {
            return Err("unknown machine work kind");
        }
    }
    Ok(result)
}

fn transfer(
    work: &mlua::Table,
    ports: &[String],
    own_slots: usize,
) -> Result<api::Work, &'static str> {
    let Value::Table(offset) = work
        .raw_get("offset")
        .map_err(|_| "machine transfer offset required")?
    else {
        return Err("machine transfer offset required");
    };
    if offset.metatable().is_some()
        || offset.raw_len() != 3
        || offset.clone().pairs::<Value, Value>().take(4).count() != 3
    {
        return Err("machine transfer offset needs three axes");
    }
    let mut normal = [0; 3];
    for (axis, coordinate) in normal.iter_mut().enumerate() {
        *coordinate = integer(
            offset
                .raw_get(axis + 1)
                .map_err(|_| "invalid transfer axis")?,
            -1,
            1,
        )? as i32;
    }
    if !api::FACES.contains(&normal) {
        return Err("machine transfer offset must be cardinal");
    }
    let own_port = text(work.raw_get("port").map_err(|_| "machine port required")?)?;
    if !ports.contains(&own_port) {
        return Err("machine transfer uses undeclared port");
    }
    let peer_port = match work.raw_get("peer_port").map_err(|_| "invalid peer port")? {
        Value::Nil => None,
        value => Some(text(value)?),
    };
    let push = match work
        .raw_get("push")
        .map_err(|_| "machine transfer direction required")?
    {
        Value::Boolean(value) => value,
        _ => return Err("machine transfer direction must be boolean"),
    };
    let count = match work
        .raw_get("count")
        .map_err(|_| "invalid transfer count")?
    {
        Value::Nil => 1,
        value => integer(value, 1, 128)? as u16,
    };
    let slot = |field: &str| -> Result<Option<u8>, &'static str> {
        match work
            .raw_get(field)
            .map_err(|_| "invalid machine transfer slot")?
        {
            Value::Nil => Ok(None),
            value => Ok(Some(integer(value, 1, 54)? as u8 - 1)),
        }
    };
    let source_slot = slot("source_slot")?;
    let destination_slot = slot("destination_slot")?;
    let item = work.raw_get("item").map_err(|_| "invalid transfer item")?;
    let same = work
        .raw_get("same_as_slot")
        .map_err(|_| "invalid transfer reference")?;
    let stack = match (item, same) {
        (Value::Nil, Value::Nil) => api::StackSelector::Any,
        (value, Value::Nil) => api::StackSelector::Item(text(value)?),
        (Value::Nil, value) => {
            api::StackSelector::SameAsSlot(integer(value, 1, own_slots as i64)? as u8 - 1)
        }
        _ => return Err("machine transfer item and same_as_slot are exclusive"),
    };
    Ok(api::Work::Transfer {
        offset: normal,
        own_port,
        peer_port,
        push,
        selection: api::TransferSelection {
            source_slot,
            destination_slot,
            stack,
            count,
        },
    })
}
