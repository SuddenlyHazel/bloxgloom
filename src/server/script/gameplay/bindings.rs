//! Borrowed callbacks are invalidated at scope exit, even if Lua retains one in
//! a module table/coroutine. Host operation and VM budgets bound all work.
use super::*;
use bloxgloom_host_api::gameplay::Cell;

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    context: &mut Context<'_>,
    event: &Event,
    rejected: &RefCell<Option<Error>>,
) -> mlua::Result<()> {
    let Event::ActionRequested {
        action,
        position,
        cell,
        entity,
        slot,
        arguments,
    } = event
    else {
        return Err(mlua::Error::RuntimeError("expected ActionRequested".into()));
    };
    let fields = lua.create_table()?;
    fields.set("kind", "ActionRequested")?;
    fields.set("action", action.as_str())?;
    fields.set("slot", *slot)?;
    fields.set("arguments", lua.create_string(arguments)?)?;
    let position = lua.create_sequence_from(*position)?;
    position.set_readonly(true);
    fields.set("position", position)?;
    if let Some(cell) = cell {
        let cell = lua.create_sequence_from(*cell)?;
        cell.set_readonly(true);
        fields.set("cell", cell)?;
    }
    if let Some(entity) = entity {
        fields.set("entity_lo", *entity as u32)?;
        fields.set("entity_hi", (*entity >> 32) as u32)?;
    }
    fields.set_readonly(true);
    let host = lua.create_table()?;
    host.set("tick_lo", context.tick() as u32)?;
    host.set("tick_hi", (context.tick() >> 32) as u32)?;
    let context = RefCell::new(context);
    // Every argument is a raw Value: decoding never calls metamethods, walks
    // script tables or allocates an unbounded Rust string. Errors are latched
    // outside the VM as well as by Context, so pcall cannot permit partial apply.
    lua.scope(|scope| {
        host.set(
            "block",
            scope.create_function(|lua, (x, y, z): (Value, Value, Value)| {
                checked(rejected, || {
                    let block = context.borrow_mut().block(cell_at(x, y, z)?)?;
                    Ok(block)
                })
                .and_then(|block| {
                    let value = lua.create_table()?;
                    value.set("state", block.state)?;
                    value.set("block_type", block.block_type)?;
                    value.set("primary_item", block.primary_item)?;
                    value.set("plant", block.plant)?;
                    value.set("supports_plant", block.supports_plant)?;
                    value.set_readonly(true);
                    Ok(value)
                })
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
        host.set_readonly(true);
        entry.call::<()>((host, fields))
    })
}

fn checked<T>(
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

fn invalid(message: &str) -> Error {
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
