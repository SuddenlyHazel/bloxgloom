//! Borrowed immutable public Context; bounded effects remain proposals until the
//! existing adapter validates the whole wave and the owner WAL receipt applies.
use super::*;
use std::cell::Cell;

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    context: &api::Context<'_>,
    max_bytes: usize,
) -> mlua::Result<api::Plan> {
    let api::Owner::Chunk(owner) = context.owner else {
        return Err(mlua::Error::RuntimeError("expected chunk owner".into()));
    };
    let host = lua.create_table()?;
    let owner = lua.create_sequence_from(owner)?;
    owner.set_readonly(true);
    host.set("owner", owner)?;
    host.set("data", lua.create_string(context.data)?)?;
    host.set("tick_lo", context.tick as u32)?;
    host.set("tick_hi", (context.tick >> 32) as u32)?;
    host.set("revision_lo", context.revision as u32)?;
    host.set("revision_hi", (context.revision >> 32) as u32)?;
    let rejected = RefCell::new(None);
    let reads = Cell::new(0usize);
    let edits = RefCell::new(Vec::new());
    let wakes = RefCell::new(Vec::new());
    lua.scope(|scope| {
        host.set(
            "block",
            scope.create_function(|lua, (x, y, z): (Value, Value, Value)| {
                checked(&rejected, || {
                    if reads.get() >= 64 {
                        return Err("system read limit exceeded (64)");
                    }
                    reads.set(reads.get() + 1);
                    context
                        .block(cell(x, y, z)?)
                        .map(|block| block.state)
                        .map_err(|_| "system world read unavailable or outside owner")
                })
                .and_then(|state| lua.create_string(state))
                .inspect_err(|_| {
                    rejected
                        .borrow_mut()
                        .get_or_insert("system world read failed");
                })
            })?,
        )?;
        host.set(
            "edit",
            scope.create_function(
                |_, (x, y, z, before, after): (Value, Value, Value, Value, Value)| {
                    checked(&rejected, || {
                        let mut edits = edits.borrow_mut();
                        if edits.len() >= 16 {
                            return Err("system edit limit exceeded (16)");
                        }
                        if context.world.is_none() {
                            return Err("system edits require read_world");
                        }
                        edits.push(api::BlockEdit {
                            cell: cell(x, y, z)?,
                            before: text(before)?,
                            after: text(after)?,
                        });
                        Ok(())
                    })
                },
            )?,
        )?;
        host.set(
            "wake",
            scope.create_function(|_, (system, x, y, z): (Value, Value, Value, Value)| {
                checked(&rejected, || {
                    let mut wakes = wakes.borrow_mut();
                    if wakes.len() >= 32 {
                        return Err("system wake limit exceeded (32)");
                    }
                    wakes.push(api::Wake {
                        system: text(system)?,
                        owner: api::Owner::Chunk(cell(x, y, z)?),
                    });
                    Ok(())
                })
            })?,
        )?;
        host.set_readonly(true);
        let (data, delay): (Value, Value) = entry.call(host)?;
        checked(&rejected, || {
            let delay = integer(delay, 1, u32::MAX.into())? as u64;
            Ok(api::Plan {
                data: bytes(data, max_bytes)?,
                next_tick: context
                    .tick
                    .checked_add(delay)
                    .ok_or("system deadline overflow")?,
                edits: std::mem::take(&mut *edits.borrow_mut()),
                wakes: std::mem::take(&mut *wakes.borrow_mut()),
            })
        })
    })
}

fn checked<T>(
    rejected: &RefCell<Option<&'static str>>,
    operation: impl FnOnce() -> Result<T, &'static str>,
) -> mlua::Result<T> {
    if let Some(error) = *rejected.borrow() {
        return Err(mlua::Error::RuntimeError(error.into()));
    }
    operation().map_err(|error| {
        *rejected.borrow_mut() = Some(error);
        mlua::Error::RuntimeError(error.into())
    })
}
