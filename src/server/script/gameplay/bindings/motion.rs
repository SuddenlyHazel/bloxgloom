//! Raw finite vectors, exact identity/revision handles, and scoped spawn refs.
use super::*;
use crate::server::script::handles;
use bloxgloom_host_api::gameplay::{MotionChange, MovingSpawn, SpawnReference};

struct SpawnRef(SpawnReference);
impl mlua::UserData for SpawnRef {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, value, ()| {
            Ok(format!("spawn-reference:{}", value.0.index()))
        });
    }
}

pub(super) fn install<'scope>(
    scope: &'scope mlua::Scope<'scope, '_>,
    host: &mlua::Table,
    context: &'scope RefCell<&mut Context<'_>>,
    rejected: &'scope RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "spawn_moving_entity",
        scope.create_function(|lua, (key, options): (Value, Value)| {
            let reference = checked(rejected, || {
                let options = table(options)?;
                let velocity =
                    optional_vector::<3>(field(&options, "velocity")?)?.unwrap_or([0.0; 3]);
                let orientation = optional_vector::<4>(field(&options, "orientation")?)?
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let source = match field(&options, "source")? {
                    Value::Nil => None,
                    value => Some(handles::entity_value(value).map_err(invalid)?),
                };
                let state = state_bytes(field(&options, "state")?)?;
                context.borrow_mut().spawn_moving_entity(MovingSpawn {
                    key: text(key).map_err(invalid)?,
                    position: vector(field(&options, "position")?)?,
                    velocity,
                    orientation,
                    state: state.as_bytes().to_vec(),
                    source,
                })
            })?;
            lua.create_userdata(SpawnRef(reference))
                .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    host.set(
        "motion",
        scope.create_function(|lua, id: Value| {
            let value = checked(rejected, || {
                context
                    .borrow_mut()
                    .motion(handles::entity_value(id).map_err(invalid)?)
            })?;
            value
                .map(|value| events::motion(lua, &value))
                .transpose()
                .inspect_err(|error| queries::latch(rejected, error))
        })?,
    )?;
    host.set(
        "set_motion",
        scope.create_function(|_, (id, revision, options): (Value, Value, Value)| {
            checked(rejected, || {
                let options = table(options)?;
                // Position is deliberately absent: steering cannot teleport a body.
                for pair in options.clone().pairs::<Value, Value>().take(4) {
                    let (Value::String(key), _) =
                        pair.map_err(|_| invalid("invalid motion field"))?
                    else {
                        return Err(invalid("invalid motion field"));
                    };
                    if !matches!(
                        key.to_str()
                            .map_err(|_| invalid("invalid motion field"))?
                            .as_ref(),
                        "velocity" | "acceleration" | "orientation"
                    ) {
                        return Err(invalid(
                            "motion changes accept only velocity, acceleration and orientation",
                        ));
                    }
                }
                let change = MotionChange {
                    velocity: optional_vector(field(&options, "velocity")?)?,
                    acceleration: optional_vector(field(&options, "acceleration")?)?,
                    orientation: optional_vector(field(&options, "orientation")?)?,
                };
                context.borrow_mut().set_motion(
                    handles::entity_value(id).map_err(invalid)?,
                    handles::revision_value(revision).map_err(invalid)?,
                    change,
                )
            })
        })?,
    )?;
    Ok(())
}

fn field(table: &mlua::Table, key: &str) -> Result<Value, Error> {
    table
        .raw_get(key)
        .map_err(|_| invalid("invalid motion field"))
}
fn table(value: Value) -> Result<mlua::Table, Error> {
    match value {
        Value::Table(table) if table.metatable().is_none() => Ok(table),
        _ => Err(invalid("motion options must be a plain table")),
    }
}
fn optional_vector<const N: usize>(value: Value) -> Result<Option<[f32; N]>, Error> {
    if value.is_nil() {
        Ok(None)
    } else {
        vector(value).map(Some)
    }
}
fn vector<const N: usize>(value: Value) -> Result<[f32; N], Error> {
    let table = table(value)?;
    if table.raw_len() != N {
        return Err(invalid("invalid motion vector length"));
    }
    let mut count = 0;
    for pair in table.clone().pairs::<Value, Value>().take(N + 1) {
        let (index, _) = pair.map_err(|_| invalid("invalid motion vector"))?;
        if !matches!(index, Value::Integer(index) if index > 0 && index as usize <= N) {
            return Err(invalid("motion vector must be dense"));
        }
        count += 1;
    }
    if count != N {
        return Err(invalid("motion vector must be dense"));
    }
    let mut result = [0.0; N];
    for (index, result) in result.iter_mut().enumerate() {
        let value = match table
            .raw_get(index + 1)
            .map_err(|_| invalid("invalid motion vector"))?
        {
            Value::Number(value) => value,
            Value::Integer(value) => value as f64,
            _ => return Err(invalid("motion vector components must be numeric")),
        };
        *result = value as f32;
        if !result.is_finite() {
            return Err(invalid("motion vectors must be finite f32"));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_vectors_reject_sparse_nonfinite_and_coerced_values() {
        let lua = Lua::new();
        for source in [
            "{1,2}",
            "{1,2,3,extra=4}",
            "{1,2,0/0}",
            "{1,2,'3'}",
            "setmetatable({1,2,3},{})",
            "{1,2,1e100}",
        ] {
            let value = lua
                .load(format!("return {source}"))
                .eval::<Value>()
                .unwrap();
            assert!(vector::<3>(value).is_err(), "{source}");
        }
        let value = lua.load("return {1,2,3}").eval::<Value>().unwrap();
        assert_eq!(vector::<3>(value).unwrap(), [1.0, 2.0, 3.0]);
        assert_eq!(optional_vector::<3>(Value::Nil).unwrap(), None);
    }
}
