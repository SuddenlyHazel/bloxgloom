//! Borrowed callbacks are invalidated at scope exit, even if Lua retains one in
//! a module table/coroutine. Host operation and VM budgets bound all work.
//! Additional dot methods: spawn_drop(x,y,z,item,count,delay_ms),
//! spawn_entity(key,x,y,z,binary_state), entity_state(id),
//! update_entity(id,binary_state), remove_entity(id),
//! schedule_entity(id,delay_ticks_or_nil). State strings are at most
//! u16::MAX bytes before host copying; the registered schema may be stricter.
//! IDs are immutable handles containing exact nonzero u64s. Delay is 1..100000 ticks; nil suspends. Host
//! ownership, schema, read dependencies and commit validation are not bypassed.
use super::*;
use bloxgloom_host_api::gameplay::Cell;
pub(in crate::server::script) mod motion;
pub(in crate::server::script) mod players;
mod profile_state;
mod queries;
mod terrain;
mod weather;

pub(in crate::server::script) fn invoke_fields<R: mlua::FromLuaMulti>(
    lua: &Lua,
    entry: Function,
    context: &mut Context<'_>,
    fields: mlua::Table,
    rejected: &RefCell<Option<Error>>,
) -> mlua::Result<R> {
    let host = lua.create_table()?;
    if let Some(profile) = context.player_profile() {
        host.set(
            "player_profile",
            crate::server::script::handles::profile(lua, profile)?,
        )?;
    }
    host.set(
        "tick",
        crate::server::script::handles::tick(lua, context.tick())?,
    )?;
    host.set("tick_lo", context.tick() as u32)?;
    host.set("tick_hi", (context.tick() >> 32) as u32)?;
    if let Some(position) = context.player_position() {
        let feet = lua.create_table()?;
        for (index, coordinate) in position.into_iter().enumerate() {
            feet.raw_set(index + 1, coordinate)?;
        }
        feet.set_readonly(true);
        host.set("player_position", feet)?;
    }
    let context = RefCell::new(context);
    // Every argument is a raw Value: decoding never calls metamethods, walks
    // script tables or allocates an unbounded Rust string. Errors are latched
    // outside the VM as well as by Context, so pcall cannot permit partial apply.
    lua.scope(|scope| {
        super::inventory::install(scope, &host, &context, rejected)?;
        queries::install(scope, &host, &context, rejected)?;
        terrain::install(scope, &host, &context, rejected)?;
        weather::install(scope, &host, &context, rejected)?;
        players::install(scope, &host, &context, rejected)?;
        profile_state::install(scope, &host, &context, rejected)?;
        motion::install(scope, &host, &context, rejected)?;
        host.set(
            "sound",
            scope.create_function(|_, value: Value| {
                checked(rejected, || {
                    let Value::Table(table) = value else {
                        return Err(invalid("expected sound table"));
                    };
                    let (voice, kind) =
                        crate::audio::luau::decode(&table).map_err(|e| invalid(&e.to_string()))?;
                    context.borrow_mut().sound(voice, kind)
                })
            })?,
        )?;
        host.set(
            "world_time",
            scope.create_function(|lua, ()| {
                let time = checked(rejected, || context.borrow_mut().world_time())?;
                (|| {
                    let value = lua.create_table()?;
                    value.set("elapsed_ms", time.elapsed_ms)?;
                    value.set("cycle_ms", time.cycle_ms)?;
                    value.set_readonly(true);
                    Ok(value)
                })()
                .inspect_err(|error: &mlua::Error| {
                    rejected
                        .borrow_mut()
                        .get_or_insert_with(|| invalid(&error.to_string()));
                })
            })?,
        )?;
        host.set(
            "admin_set_time",
            scope.create_function(|_, value: Value| {
                checked(rejected, || {
                    context.borrow_mut().admin_set_time(
                        integer(value, 0, (crate::daylight::CYCLE_MS - 1) as i64)
                            .map_err(invalid)? as u64,
                    )
                })
            })?,
        )?;
        // These are public host operations, not a script-selected admin token.
        // Every invocation checks the server-selected local admin profile before staging.
        host.set(
            "admin_give",
            scope.create_function(|_, (item, count): (Value, Value)| {
                checked(rejected, || {
                    context.borrow_mut().admin_give(
                        &text(item).map_err(invalid)?,
                        integer(count, 1, 128).map_err(invalid)? as u16,
                    )
                })
            })?,
        )?;
        host.set(
            "admin_spawn",
            scope.create_function(|_, key: Value| {
                checked(rejected, || {
                    context
                        .borrow_mut()
                        .admin_spawn(&text(key).map_err(invalid)?)
                })
            })?,
        )?;
        host.set(
            "block",
            scope.create_function(|lua, (x, y, z): (Value, Value, Value)| {
                checked(rejected, || {
                    let block = context.borrow_mut().block(cell_at(x, y, z)?)?;
                    Ok(block)
                })
                .and_then(|block| events::block(lua, &block))
                .inspect_err(|error| {
                    // Also latch errors constructing the VM result (e.g. its
                    // memory ceiling), without replacing a retryable host error.
                    rejected
                        .borrow_mut()
                        .get_or_insert_with(|| invalid(&error.to_string()));
                })
            })?,
        )?;
        host.set(
            "set_block",
            scope.create_function(|_, (x, y, z, key): (Value, Value, Value, Value)| {
                checked(rejected, || {
                    context
                        .borrow_mut()
                        .set_block(cell_at(x, y, z)?, &text(key).map_err(invalid)?)
                })
            })?,
        )?;
        host.set(
            "transfer",
            scope.create_function(|_, (source, destination, count): (Value, Value, Value)| {
                checked(rejected, || {
                    let source = integer(source, 0, 255).map_err(invalid)? as usize;
                    let destination = integer(destination, 0, 255).map_err(invalid)? as usize;
                    let count = integer(count, 1, 128).map_err(invalid)? as u16;
                    let mut context = context.borrow_mut();
                    let player = context
                        .player()
                        .ok_or_else(|| invalid("action has no player"))?;
                    context.transfer(player, source, player, destination, count)
                })
            })?,
        )?;
        host.set(
            "spawn_drop",
            scope.create_function(
                |_, (x, y, z, item, count, delay): (Value, Value, Value, Value, Value, Value)| {
                    checked(rejected, || {
                        context.borrow_mut().spawn_drop(
                            position_at(x, y, z)?,
                            &text(item).map_err(invalid)?,
                            integer(count, 1, 128).map_err(invalid)? as u16,
                            integer(delay, 0, u32::MAX.into()).map_err(invalid)? as u32,
                        )
                    })
                },
            )?,
        )?;
        host.set(
            "spawn_stack",
            scope.create_function(
                |_, (x, y, z, stack, delay): (Value, Value, Value, Value, Value)| {
                    checked(rejected, || {
                        context.borrow_mut().spawn_stack(
                            position_at(x, y, z)?,
                            super::inventory::decode_stack(stack)?,
                            integer(delay, 0, u32::MAX.into()).map_err(invalid)? as u32,
                        )
                    })
                },
            )?,
        )?;
        host.set(
            "spawn_entity",
            scope.create_function(
                |_, (key, x, y, z, state): (Value, Value, Value, Value, Value)| {
                    checked(rejected, || {
                        context.borrow_mut().spawn_entity(
                            &text(key).map_err(invalid)?,
                            position_at(x, y, z)?,
                            &state_bytes(state)?.as_bytes(),
                        )
                    })
                },
            )?,
        )?;
        host.set(
            "entity_state",
            scope.create_function(|lua, (lo, hi): (Value, Value)| {
                checked(rejected, || {
                    context.borrow_mut().entity_state(entity_id(lo, hi)?)
                })
                .and_then(|state| state.map(|state| lua.create_string(state)).transpose())
                .inspect_err(|error| {
                    rejected
                        .borrow_mut()
                        .get_or_insert_with(|| invalid(&error.to_string()));
                })
            })?,
        )?;
        host.set(
            "update_entity",
            scope.create_function(|_, (lo, hi, state): (Value, Value, Value)| {
                checked(rejected, || {
                    let (id, state) = if matches!(lo, Value::UserData(_)) && state.is_nil() {
                        (entity_id(lo, Value::Nil)?, hi)
                    } else {
                        (entity_id(lo, hi)?, state)
                    };
                    context
                        .borrow_mut()
                        .update_entity(id, &state_bytes(state)?.as_bytes())
                })
            })?,
        )?;
        host.set(
            "remove_entity",
            scope.create_function(|_, (lo, hi): (Value, Value)| {
                checked(rejected, || {
                    context.borrow_mut().remove_entity(entity_id(lo, hi)?)
                })
            })?,
        )?;
        host.set(
            "schedule_entity",
            scope.create_function(|_, (lo, hi, delay): (Value, Value, Value)| {
                checked(rejected, || {
                    let (id, delay) = if matches!(lo, Value::UserData(_)) && delay.is_nil() {
                        (entity_id(lo, Value::Nil)?, hi)
                    } else {
                        (entity_id(lo, hi)?, delay)
                    };
                    let delay = if delay.is_nil() {
                        None
                    } else {
                        Some(integer(delay, 1, 100_000).map_err(invalid)? as u32)
                    };
                    context.borrow_mut().schedule_entity(id, delay)
                })
            })?,
        )?;
        host.set_readonly(true);
        let tags = context.borrow().tags();
        super::super::reads::tags_with(lua, &host, tags.as_deref(), || {
            entry.call::<R>((host.clone(), fields))
        })
    })
}

pub(super) fn checked<T>(
    rejected: &RefCell<Option<Error>>,
    operation: impl FnOnce() -> Result<T, Error>,
) -> mlua::Result<T> {
    if let Some(error) = rejected.borrow().as_ref() {
        return Err(mlua::Error::external(error.clone()));
    }
    operation().map_err(|error| {
        *rejected.borrow_mut() = Some(error.clone());
        mlua::Error::external(error)
    })
}

pub(super) fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}

fn cell_at(x: Value, y: Value, z: Value) -> Result<Cell, Error> {
    let axis = |v| {
        integer(v, i64::from(i32::MIN), i64::from(i32::MAX))
            .map(|v| v as i32)
            .map_err(invalid)
    };
    Ok([axis(x)?, axis(y)?, axis(z)?])
}

pub(super) fn entity_id(lo: Value, hi: Value) -> Result<u64, Error> {
    if matches!(lo, Value::UserData(_)) {
        if !hi.is_nil() {
            return Err(invalid("entity handle takes one argument"));
        }
        return crate::server::script::handles::entity_value(lo).map_err(invalid);
    }
    let lo = integer(lo, 0, u32::MAX.into()).map_err(invalid)? as u64;
    let hi = integer(hi, 0, u32::MAX.into()).map_err(invalid)? as u64;
    let id = lo | (hi << 32);
    if id == 0 {
        return Err(invalid("entity ID must be nonzero"));
    }
    Ok(id)
}

fn state_bytes(value: Value) -> Result<mlua::LuaString, Error> {
    let Value::String(value) = value else {
        return Err(invalid("expected binary entity state string"));
    };
    if value.as_bytes().len() > usize::from(u16::MAX) {
        return Err(invalid("entity state byte limit exceeded"));
    }
    Ok(value)
}

fn position_at(x: Value, y: Value, z: Value) -> Result<[f32; 3], Error> {
    let axis = |v| {
        let value = match v {
            Value::Integer(v) => v as f64,
            Value::Number(v) => v,
            _ => return Err(invalid("expected numeric position")),
        };
        let value = value as f32;
        if !value.is_finite() {
            return Err(invalid("position must be finite f32"));
        }
        Ok(value)
    };
    Ok([axis(x)?, axis(y)?, axis(z)?])
}
