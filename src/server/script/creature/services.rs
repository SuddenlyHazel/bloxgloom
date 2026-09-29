//! Bounded, read-only world services for a creature tick. A failed call
//! poisons the invocation even when the package catches its Lua error.
use super::*;
use crate::server::script::values::integer;
use bloxgloom_host_api::entity as api;
use mlua::Value;
use std::cell::{Cell, RefCell};

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    context: &api::Context<'_>,
    private: &[u8],
    max_private: usize,
) -> mlua::Result<(Vec<u8>, u32, Option<[f32; 3]>)> {
    let host = lua.create_table()?;
    host.set("event", "tick")?;
    host.set("data", lua.create_string(private)?)?;
    host.set("id_lo", context.id as u32)?;
    host.set("id_hi", (context.id >> 32) as u32)?;
    host.set("tick_lo", context.tick as u32)?;
    host.set("tick_hi", (context.tick >> 32) as u32)?;
    let position = lua.create_sequence_from(context.position)?;
    position.set_readonly(true);
    host.set("position", position)?;
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
        let (data, delay, x, z): (Value, Value, Value, Value) = entry.call(host)?;
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
        Ok((data.as_bytes().to_vec(), delay, target))
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
