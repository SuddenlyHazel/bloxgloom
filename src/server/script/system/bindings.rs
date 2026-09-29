//! Borrowed immutable public Context; bounded effects remain proposals until the
//! existing adapter validates the whole wave and the owner WAL receipt applies.
use super::*;
use std::cell::Cell;
mod intents;

#[derive(Clone, Copy)]
pub(super) struct Capabilities {
    pub drops: bool,
    pub entities: bool,
}

pub(super) fn invoke(
    lua: &Lua,
    entry: Function,
    context: &api::Context<'_>,
    max_bytes: usize,
    capabilities: Capabilities,
    inbox: &[api::IntentDelivery],
    outbox: Option<&mut api::IntentOutbox>,
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
    host.set("inbox", intents::inbox(lua, inbox)?)?;
    if let Some(entities) = context.entities() {
        let list = lua.create_table_with_capacity(entities.len(), 0)?;
        for (index, entity) in entities.iter().enumerate() {
            let value = lua.create_table()?;
            value.set("id_lo", entity.id as u32)?;
            value.set("id_hi", (entity.id >> 32) as u32)?;
            value.set("revision_lo", entity.revision as u32)?;
            value.set("revision_hi", (entity.revision >> 32) as u32)?;
            value.set("key", entity.key.as_str())?;
            let position = lua.create_sequence_from(entity.position)?;
            position.set_readonly(true);
            value.set("position", position)?;
            value.set("state", lua.create_string(&entity.state)?)?;
            value.set_readonly(true);
            list.raw_set(index + 1, value)?;
        }
        list.set_readonly(true);
        host.set("entities", list)?;
    }
    let rejected = RefCell::new(None);
    let reads = Cell::new(0usize);
    let edits = RefCell::new(Vec::new());
    let drops = RefCell::new(Vec::new());
    let entity_spawns = RefCell::new(Vec::new());
    let wakes = RefCell::new(Vec::new());
    let outbox = RefCell::new(outbox);
    lua.scope(|scope| {
        host.set(
            "send",
            scope.create_function(|_, (x, y, z, payload): (Value, Value, Value, Value)| {
                checked(&rejected, || {
                    let mut outbox = outbox.borrow_mut();
                    let outbox = outbox
                        .as_mut()
                        .ok_or("system must declare accepts_intents")?;
                    let destination = api::Owner::Chunk(cell(x, y, z)?);
                    let Value::String(payload) = payload else {
                        return Err("intent payload must be a binary string");
                    };
                    // The public outbox checks count/byte bounds before copying.
                    outbox
                        .send(destination, &payload.as_bytes())
                        .map_err(|_| "system intent outbox exceeds bound")
                })
            })?,
        )?;
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
                        .map_err(
                            |_| "system world read unavailable or outside declared neighborhood",
                        )
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
                        let cell = cell(x, y, z)?;
                        let before = text(before)?;
                        let after = text(after)?;
                        // Validate against the immutable capture, not live terrain
                        // or earlier proposals. Even caught errors poison output.
                        let captured = context.block(cell).map_err(
                            |_| "system edit unavailable or outside declared neighborhood",
                        )?;
                        if captured.state != before {
                            return Err("system edit before-value differs from capture");
                        }
                        edits.push(api::BlockEdit {
                            cell,
                            before,
                            after,
                        });
                        Ok(())
                    })
                },
            )?,
        )?;
        host.set(
            "spawn_drop",
            scope.create_function(
                |_, (x, y, z, item, count, delay): (Value, Value, Value, Value, Value, Value)| {
                    checked(&rejected, || {
                        if !capabilities.drops {
                            return Err("system has not declared drop creation");
                        }
                        let mut drops = drops.borrow_mut();
                        if drops.len() >= 16 {
                            return Err("system drop limit exceeded (16)");
                        }
                        let cell = cell(x, y, z)?;
                        if cell.iter().any(|v| v.unsigned_abs() > 1_000_000) {
                            return Err("system drop coordinate exceeds exact range");
                        }
                        context
                            .block(cell)
                            .map_err(|_| "system drop cell unavailable")?;
                        drops.push(api::DropSpawn {
                            position: cell.map(|v| v as f32 + 0.5),
                            item: text(item)?,
                            count: integer(count, 1, 128)? as u16,
                            pickup_delay_ms: integer(delay, 0, u32::MAX.into())? as u32,
                        });
                        Ok(())
                    })
                },
            )?,
        )?;
        host.set(
            "spawn_entity",
            scope.create_function(
                |_, (key, x, y, z, state): (Value, Value, Value, Value, Value)| {
                    checked(&rejected, || {
                        if !capabilities.entities {
                            return Err("system has not declared entity creation");
                        }
                        let mut spawns = entity_spawns.borrow_mut();
                        if spawns.len() >= 16 {
                            return Err("system entity spawn limit exceeded (16)");
                        }
                        let cell = cell(x, y, z)?;
                        if cell.iter().any(|v| v.unsigned_abs() > 1_000_000) {
                            return Err("system entity coordinate exceeds exact range");
                        }
                        context
                            .block(cell)
                            .map_err(|_| "system entity cell unavailable")?;
                        let Value::String(state) = state else {
                            return Err("system entity state must be a binary string");
                        };
                        if state.as_bytes().len() > 1024 {
                            return Err("system entity state exceeds 1024 bytes");
                        }
                        spawns.push(api::EntitySpawn {
                            position: cell.map(|v| v as f32 + 0.5),
                            key: text(key)?,
                            state: state.as_bytes().to_vec(),
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
                drops: std::mem::take(&mut *drops.borrow_mut()),
                entity_spawns: std::mem::take(&mut *entity_spawns.borrow_mut()),
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
