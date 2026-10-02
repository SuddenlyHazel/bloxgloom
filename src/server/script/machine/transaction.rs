//! Parse custom machine processing with exact input preimages captured by the
//! host. Scripts choose counts and outputs, never supply trusted input snapshots.
use super::*;
use crate::server::script::values::text;

fn records(value: Value, minimum: usize) -> Result<Vec<mlua::Table>, &'static str> {
    let Value::Table(table) = value else {
        return Err("machine transformation inputs/outputs must be lists");
    };
    let count = table.raw_len();
    if table.metatable().is_some()
        || !(minimum..=8).contains(&count)
        || table.clone().pairs::<Value, Value>().take(9).count() != count
    {
        return Err("machine transformation lists must be dense, bounded to eight");
    }
    (1..=count)
        .map(|index| {
            let record: mlua::Table = table
                .raw_get(index)
                .map_err(|_| "invalid transform record")?;
            if record.metatable().is_some() {
                return Err("machine transformation record cannot have a metatable");
            }
            Ok(record)
        })
        .collect()
}

fn field(table: &mlua::Table, key: &str) -> Result<Value, &'static str> {
    table
        .raw_get(key)
        .map_err(|_| "invalid transformation field")
}

pub(super) fn parse(
    work: &mlua::Table,
    slots: &[Option<api::Slot<'_>>],
) -> Result<api::Transformation, &'static str> {
    let slot = |table: &mlua::Table| {
        integer(field(table, "slot")?, 1, slots.len() as i64).map(|value| value as u8 - 1)
    };
    let mut inputs = Vec::new();
    for record in records(field(work, "inputs")?, 1)? {
        let index = slot(&record)?;
        let captured = slots[usize::from(index)]
            .as_ref()
            .ok_or("machine transformation input slot is empty")?;
        let count = integer(field(&record, "count")?, 1, i64::from(captured.count))? as u16;
        inputs.push(api::Input {
            slot: index,
            count,
            expected: api::StackValue {
                item: captured.item.into(),
                count: captured.count,
                components: captured.components.clone(),
            },
        });
    }
    let mut outputs = Vec::new();
    for record in records(field(work, "outputs")?, 0)? {
        let index = slot(&record)?;
        let components = match field(&record, "components")? {
            Value::Nil => None,
            Value::Table(value) if value.metatable().is_none() => {
                let version = integer(field(&value, "version")?, 1, u16::MAX.into())? as u16;
                let Value::String(bytes) = field(&value, "bytes")? else {
                    return Err("transformation component bytes must be a binary string");
                };
                if !(1..=1024).contains(&bytes.as_bytes().len()) {
                    return Err("transformation component bytes must be 1..=1024 bytes");
                }
                Some(api::ComponentValue {
                    version,
                    bytes: bytes.as_bytes().to_vec(),
                })
            }
            _ => return Err("invalid transformation components"),
        };
        outputs.push(api::Output {
            slot: index,
            stack: api::StackValue {
                item: text(field(&record, "item")?)?,
                count: integer(field(&record, "count")?, 1, 128)? as u16,
                components,
            },
        });
    }
    let result = api::Transformation { inputs, outputs };
    if !result.valid(slots.len()) {
        return Err("invalid transformation slots or stacks");
    }
    Ok(result)
}
