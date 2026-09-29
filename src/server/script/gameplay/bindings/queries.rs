//! Public, dependency-capturing gameplay reads. Entities expose only their
//! registered projection; private state still requires ownership checks.
use super::*;
use bloxgloom_host_api::gameplay::Entity;
use mlua::{Scope, Table};

pub(super) fn install<'scope, 'env: 'scope>(
    scope: &'scope Scope<'scope, 'env>,
    host: &Table,
    context: &'env RefCell<&mut Context<'_>>,
    rejected: &'env RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "random",
        scope.create_function(
            |_, (x, y, z, lo, hi): (Value, Value, Value, Value, Value)| {
                let legacy = !hi.is_nil();
                checked(rejected, || {
                    let sequence = if legacy {
                        integer(lo, 0, u32::MAX.into()).map_err(invalid)? as u64
                            | ((integer(hi, 0, u32::MAX.into()).map_err(invalid)? as u64) << 32)
                    } else if lo.is_nil() {
                        0
                    } else {
                        integer(lo, 0, u32::MAX.into()).map_err(invalid)? as u64
                    };
                    context.borrow_mut().random(cell_at(x, y, z)?, sequence)
                })
                .map(|word| {
                    if legacy {
                        (
                            Value::Integer((word as u32).into()),
                            Value::Integer(((word >> 32) as u32).into()),
                        )
                    } else {
                        (
                            Value::Number(crate::server::script::handles::unit_random(word)),
                            Value::Nil,
                        )
                    }
                })
            },
        )?,
    )?;
    host.set(
        "entity",
        scope.create_function(|lua, (lo, hi): (Value, Value)| {
            checked(rejected, || context.borrow_mut().entity(entity_id(lo, hi)?))
                .and_then(|entity| entity.map(|entity| view(lua, &entity)).transpose())
                .inspect_err(|error| latch(rejected, error))
        })?,
    )?;
    host.set(
        "nearby_entities",
        scope.create_function(|lua, (x, y, z, radius): (Value, Value, Value, Value)| {
            let entities = checked(rejected, || {
                context
                    .borrow_mut()
                    .nearby_entities(position_at(x, y, z)?, number(radius)?)
            })?;
            let result = lua
                .create_table_with_capacity(entities.len(), 0)
                .inspect_err(|error| latch(rejected, error))?;
            for (index, entity) in entities.iter().enumerate() {
                result
                    .raw_set(
                        index + 1,
                        view(lua, entity).inspect_err(|error| latch(rejected, error))?,
                    )
                    .inspect_err(|error| latch(rejected, error))?;
            }
            result.set_readonly(true);
            Ok(result)
        })?,
    )?;
    host.set(
        "anchored_entity_at",
        scope.create_function(|lua, (x, y, z): (Value, Value, Value)| {
            checked(rejected, || {
                context.borrow_mut().anchored_entity_at(cell_at(x, y, z)?)
            })
            .and_then(|id| {
                id.map(|id| crate::server::script::handles::entity(lua, id))
                    .transpose()
            })
            .inspect_err(|error| latch(rejected, error))
        })?,
    )?;
    Ok(())
}

fn number(value: Value) -> Result<f32, Error> {
    let number = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err(invalid("expected numeric radius")),
    };
    let number = number as f32;
    if !number.is_finite() {
        return Err(invalid("radius must be finite"));
    }
    Ok(number)
}

fn view(lua: &Lua, entity: &Entity) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    result.set(
        "id",
        crate::server::script::handles::entity(lua, entity.id)?,
    )?;
    result.set("id_lo", entity.id as u32)?;
    result.set("id_hi", (entity.id >> 32) as u32)?;
    result.set("entity_type", entity.entity_type.as_str())?;
    result.set("data", lua.create_string(&entity.data)?)?;
    let position = lua.create_sequence_from(entity.position)?;
    position.set_readonly(true);
    result.set("position", position)?;
    if let Some(anchor) = entity.anchor {
        let anchor = lua.create_sequence_from(anchor)?;
        anchor.set_readonly(true);
        result.set("anchor", anchor)?;
    }
    result.set_readonly(true);
    Ok(result)
}

fn latch(rejected: &RefCell<Option<Error>>, error: &mlua::Error) {
    rejected
        .borrow_mut()
        .get_or_insert_with(|| invalid(&error.to_string()));
}
