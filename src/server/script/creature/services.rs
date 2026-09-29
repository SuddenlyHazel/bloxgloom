//! Bounded, read-only world services for a creature tick. A failed call
//! poisons the invocation even when the package catches its Lua error.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::entity as api;
use mlua::Value;
use std::cell::{Cell, RefCell};

pub(super) struct LifecycleRequest {
    pub despawn: bool,
    pub spawns: Vec<SpawnRequest>,
}

pub(super) struct SpawnRequest {
    pub key: Option<String>,
    pub position: [f32; 3],
}

pub(super) struct TickResult {
    pub private: Vec<u8>,
    pub delay: u32,
    pub target: Option<[f32; 3]>,
    pub lifecycle: LifecycleRequest,
}

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    context: &api::Context<'_>,
    private: &[u8],
    max_private: usize,
) -> mlua::Result<TickResult> {
    let host = lua.create_table()?;
    host.set("event", "tick")?;
    host.set("data", lua.create_string(private)?)?;
    host.set(
        "id",
        crate::server::script::handles::entity(lua, context.id)?,
    )?;
    host.set("id_lo", context.id as u32)?;
    host.set("id_hi", (context.id >> 32) as u32)?;
    host.set(
        "tick",
        crate::server::script::handles::tick(lua, context.tick)?,
    )?;
    host.set("tick_lo", context.tick as u32)?;
    host.set("tick_hi", (context.tick >> 32) as u32)?;
    let position = lua.create_sequence_from(context.position)?;
    position.set_readonly(true);
    host.set("position", position)?;
    let neighbours = lua.create_table()?;
    for (index, neighbour) in context.neighbours.iter().enumerate() {
        let record = lua.create_table()?;
        record.set(
            "id",
            crate::server::script::handles::entity(lua, neighbour.id)?,
        )?;
        record.set("id_lo", neighbour.id as u32)?;
        record.set("id_hi", (neighbour.id >> 32) as u32)?;
        record.set("key", neighbour.key)?;
        let position = lua.create_sequence_from(neighbour.position)?;
        position.set_readonly(true);
        record.set("position", position)?;
        record.set("public", lua.create_string(neighbour.public)?)?;
        record.set_readonly(true);
        neighbours.raw_set(index + 1, record)?;
    }
    neighbours.set_readonly(true);
    host.set("neighbours", neighbours)?;
    let calls = Cell::new(0u8);
    let routes = Cell::new(0u8);
    let rejected = RefCell::new(None);
    lua.scope(|scope| {
        host.set(
            "route",
            scope.create_function(|lua, (x, z): (Value, Value)| {
                guarded(&calls, &rejected, || {
                    if routes.get() >= 8 {
                        return Err(invalid("creature route limit exceeded"));
                    }
                    routes.set(routes.get() + 1);
                    let x = integer(x, -1_000_000, 1_000_000).map_err(invalid)? as i32;
                    let z = integer(z, -1_000_000, 1_000_000).map_err(invalid)? as i32;
                    match context
                        .world
                        .route(context.position, [x, z])
                        .map_err(|_| invalid("creature route unavailable"))?
                    {
                        api::Route::Next(point) => {
                            let result = lua.create_sequence_from(point)?;
                            result.set_readonly(true);
                            Ok(Value::Table(result))
                        }
                        api::Route::Arrived | api::Route::Unreachable => Ok(Value::Nil),
                        api::Route::BudgetExhausted => {
                            Err(invalid("creature route budget exceeded"))
                        }
                    }
                })
            })?,
        )?;
        host.set(
            "solid",
            scope.create_function(|_, (x, y, z): (Value, Value, Value)| {
                guarded(&calls, &rejected, || {
                    let cell = [integer_cell(x)?, integer_cell(y)?, integer_cell(z)?];
                    context
                        .world
                        .solid(cell)
                        .map_err(|_| invalid("creature solid unavailable"))
                })
            })?,
        )?;
        host.set(
            "clear",
            scope.create_function(|_, (x, y, z): (Value, Value, Value)| {
                guarded(&calls, &rejected, || {
                    context
                        .world
                        .clear([coordinate(x)?, coordinate(y)?, coordinate(z)?])
                        .map_err(|_| invalid("creature clear unavailable"))
                })
            })?,
        )?;
        host.set(
            "grounded",
            scope.create_function(|_, (x, y, z): (Value, Value, Value)| {
                guarded(&calls, &rejected, || {
                    context
                        .world
                        .grounded([coordinate(x)?, coordinate(y)?, coordinate(z)?])
                        .map_err(|_| invalid("creature grounded unavailable"))
                })
            })?,
        )?;
        host.set(
            "walk_edge",
            scope.create_function(
                |_, (x1, y1, z1, x2, y2, z2): (Value, Value, Value, Value, Value, Value)| {
                    guarded(&calls, &rejected, || {
                        let from = [coordinate(x1)?, coordinate(y1)?, coordinate(z1)?];
                        let to = [coordinate(x2)?, coordinate(y2)?, coordinate(z2)?];
                        context
                            .world
                            .walk_edge(from, to)
                            .map_err(|_| invalid("creature walk edge unavailable"))
                    })
                },
            )?,
        )?;
        host.set_readonly(true);
        let (data, delay, x, z, lifecycle): (Value, Value, Value, Value, Value) =
            entry.call(host)?;
        if let Some(error) = *rejected.borrow() {
            return Err(invalid(error));
        }
        let Value::String(data) = data else {
            return Err(invalid("creature state must be binary string"));
        };
        if data.as_bytes().len() > max_private {
            return Err(invalid("creature state exceeds bound"));
        }
        let delay = integer(delay, 1, 100_000).map_err(invalid)? as u32;
        let target = if x.is_nil() && z.is_nil() {
            None
        } else if !x.is_nil() && !z.is_nil() {
            Some([coordinate(x)?, context.position[1], coordinate(z)?])
        } else {
            return Err(invalid("creature target needs x and z"));
        };
        Ok(TickResult {
            private: data.as_bytes().to_vec(),
            delay,
            target,
            lifecycle: parse_lifecycle(lifecycle, context.position)?,
        })
    })
}

fn parse_lifecycle(value: Value, origin: [f32; 3]) -> mlua::Result<LifecycleRequest> {
    let Value::Table(table) = value else {
        return if value.is_nil() {
            Ok(LifecycleRequest {
                despawn: false,
                spawns: vec![],
            })
        } else {
            Err(invalid("creature lifecycle must be a table"))
        };
    };
    if table.metatable().is_some() {
        return Err(invalid("creature lifecycle metatable forbidden"));
    }
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        if !matches!(key, Value::String(ref s) if s.as_bytes().as_ref() == b"despawn" || s.as_bytes().as_ref() == b"spawns")
        {
            return Err(invalid("unknown creature lifecycle field"));
        }
    }
    let despawn = match table.raw_get::<Value>("despawn")? {
        Value::Nil | Value::Boolean(false) => false,
        Value::Boolean(true) => true,
        _ => return Err(invalid("creature despawn must be boolean")),
    };
    let spawns = match table.raw_get::<Value>("spawns")? {
        Value::Nil => vec![],
        Value::Table(spawns) if spawns.metatable().is_none() => {
            let count = spawns.raw_len();
            if count > 4 || spawns.clone().pairs::<Value, Value>().count() != count {
                return Err(invalid(
                    "creature spawns must be a dense list of at most four",
                ));
            }
            let mut requests = Vec::with_capacity(count);
            for index in 1..=count {
                let Value::Table(point) = spawns.raw_get::<Value>(index)? else {
                    return Err(invalid("creature spawn must be a table"));
                };
                let request = parse_spawn(point)?;
                let position = request.position;
                if (0..3).any(|axis| (position[axis] - origin[axis]).abs() > 8.0) {
                    return Err(invalid("creature spawn position exceeds local bound"));
                }
                requests.push(request);
            }
            requests
        }
        _ => return Err(invalid("creature spawns must be a dense list")),
    };
    Ok(LifecycleRequest { despawn, spawns })
}

fn parse_spawn(point: mlua::Table) -> mlua::Result<SpawnRequest> {
    if point.metatable().is_some() {
        return Err(invalid("creature spawn metatable forbidden"));
    }
    let (key, coordinates) = if point.raw_len() == 3 {
        if point.clone().pairs::<Value, Value>().count() != 3 {
            return Err(invalid(
                "creature spawn position must have three coordinates",
            ));
        }
        (None, point)
    } else {
        for pair in point.clone().pairs::<Value, Value>() {
            let (field, _) = pair?;
            if !matches!(field, Value::String(ref s) if s.as_bytes().as_ref() == b"key" || s.as_bytes().as_ref() == b"position")
            {
                return Err(invalid("unknown creature spawn field"));
            }
        }
        let Value::String(key) = point.raw_get::<Value>("key")? else {
            return Err(invalid("creature spawn key required"));
        };
        let key = key.to_str()?.to_owned();
        if key.len() > 129 {
            return Err(invalid("creature spawn key too long"));
        }
        let Value::Table(position) = point.raw_get::<Value>("position")? else {
            return Err(invalid("creature spawn position required"));
        };
        if position.metatable().is_some()
            || position.raw_len() != 3
            || position.clone().pairs::<Value, Value>().count() != 3
        {
            return Err(invalid(
                "creature spawn position must have three coordinates",
            ));
        }
        (Some(key), position)
    };
    Ok(SpawnRequest {
        key,
        position: [
            coordinate(coordinates.raw_get(1)?)?,
            coordinate(coordinates.raw_get(2)?)?,
            coordinate(coordinates.raw_get(3)?)?,
        ],
    })
}

fn guarded<T>(
    calls: &Cell<u8>,
    rejected: &RefCell<Option<&'static str>>,
    action: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    if let Some(error) = *rejected.borrow() {
        return Err(invalid(error));
    }
    let result = if calls.get() >= 16 {
        Err(invalid("creature world query limit exceeded"))
    } else {
        calls.set(calls.get() + 1);
        action()
    };
    if result.is_err() {
        rejected
            .borrow_mut()
            .get_or_insert("creature world query failed");
    }
    result
}

fn coordinate(value: Value) -> mlua::Result<f32> {
    let number = match value {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => return Err(invalid("creature coordinate must be numeric")),
    };
    if !number.is_finite() || !(-1_000_000.0..=1_000_000.0).contains(&number) {
        return Err(invalid("creature coordinate out of bounds"));
    }
    Ok(number as f32)
}

fn integer_cell(value: Value) -> mlua::Result<i32> {
    integer(value, -1_000_000, 1_000_000)
        .map(|value| value as i32)
        .map_err(invalid)
}
