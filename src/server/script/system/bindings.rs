//! Borrowed immutable public Context; bounded effects remain proposals until the
//! existing adapter validates the whole wave and the owner WAL receipt applies.
use super::*;
use std::cell::Cell;
mod intents;

#[derive(Clone, Copy)]
pub(super) struct Capabilities {
    pub drops: bool,
    pub entities: bool,
    pub entity_mutations: bool,
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
    let host = lua.create_table()?;
    let (kind, owner) = super::owners::present(lua, context.owner)?;
    host.set("owner", owner)?;
    host.set("owner_kind", kind)?;
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
    let entity_changes = RefCell::new(Vec::new());
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
            "update_entity",
            scope.create_function(
                |_,
                 (id_lo, id_hi, revision_lo, revision_hi, state): (
                    Value,
                    Value,
                    Value,
                    Value,
                    Value,
                )| {
                    checked(&rejected, || {
                        if !capabilities.entity_mutations {
                            return Err("system has not declared entity mutation");
                        }
                        let (id, revision) =
                            entity_identity(id_lo, id_hi, revision_lo, revision_hi)?;
                        captured_entity(context, id, revision)?;
                        let Value::String(state) = state else {
                            return Err("entity state must be a binary string");
                        };
                        let mut changes = entity_changes.borrow_mut();
                        if changes.len() >= 16
                            || changes
                                .iter()
                                .any(|change: &api::EntityChange| change.id() == id)
                        {
                            return Err("duplicate or excessive entity mutation");
                        }
                        if state.as_bytes().len() > 1024 {
                            return Err("entity state exceeds 1024 bytes");
                        }
                        changes.push(api::EntityChange::Update {
                            id,
                            before_revision: revision,
                            state: state.as_bytes().to_vec(),
                        });
                        Ok(())
                    })
                },
            )?,
        )?;
        host.set(
            "remove_entity",
            scope.create_function(
                |_, (id_lo, id_hi, revision_lo, revision_hi): (Value, Value, Value, Value)| {
                    checked(&rejected, || {
                        if !capabilities.entity_mutations {
                            return Err("system has not declared entity mutation");
                        }
                        let (id, revision) =
                            entity_identity(id_lo, id_hi, revision_lo, revision_hi)?;
                        captured_entity(context, id, revision)?;
                        let mut changes = entity_changes.borrow_mut();
                        if changes.len() >= 16
                            || changes
                                .iter()
                                .any(|change: &api::EntityChange| change.id() == id)
                        {
                            return Err("duplicate or excessive entity mutation");
                        }
                        changes.push(api::EntityChange::Remove {
                            id,
                            before_revision: revision,
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
                    push_wake(&wakes, system, api::Owner::Chunk(cell(x, y, z)?))
                })
            })?,
        )?;
        host.set(
            "wake_entity",
            scope.create_function(|_, (system, lo, hi): (Value, Value, Value)| {
                checked(&rejected, || {
                    push_wake(
                        &wakes,
                        system,
                        api::Owner::Entity(super::owners::entity(
                            super::owners::word(lo)?,
                            super::owners::word(hi)?,
                        )),
                    )
                })
            })?,
        )?;
        host.set(
            "wake_profile",
            scope.create_function(
                |_, (system, a, b, c, d): (Value, Value, Value, Value, Value)| {
                    checked(&rejected, || {
                        push_wake(
                            &wakes,
                            system,
                            api::Owner::Profile(super::owners::profile([
                                super::owners::word(a)?,
                                super::owners::word(b)?,
                                super::owners::word(c)?,
                                super::owners::word(d)?,
                            ])),
                        )
                    })
                },
            )?,
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
                entity_changes: std::mem::take(&mut *entity_changes.borrow_mut()),
                wakes: std::mem::take(&mut *wakes.borrow_mut()),
            })
        })
    })
}

fn push_wake(
    wakes: &RefCell<Vec<api::Wake>>,
    system: Value,
    owner: api::Owner,
) -> Result<(), &'static str> {
    let mut wakes = wakes.borrow_mut();
    if wakes.len() >= 32 {
        return Err("system wake limit exceeded (32)");
    }
    wakes.push(api::Wake {
        system: text(system)?,
        owner,
    });
    Ok(())
}

fn entity_identity(
    id_lo: Value,
    id_hi: Value,
    revision_lo: Value,
    revision_hi: Value,
) -> Result<(u64, u64), &'static str> {
    let half = |value| integer(value, 0, u32::MAX.into()).map(|value| value as u64);
    let id = half(id_lo)? | (half(id_hi)? << 32);
    let revision = half(revision_lo)? | (half(revision_hi)? << 32);
    if id == 0 {
        return Err("entity ID must be nonzero");
    }
    Ok((id, revision))
}

fn captured_entity(context: &api::Context<'_>, id: u64, revision: u64) -> Result<(), &'static str> {
    if context.entities().is_some_and(|entities| {
        entities
            .binary_search_by_key(&id, |entity| entity.id)
            .ok()
            .and_then(|index| entities.get(index))
            .is_some_and(|entity| entity.revision == revision)
    }) {
        Ok(())
    } else {
        Err("entity is absent or differs from capture")
    }
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
