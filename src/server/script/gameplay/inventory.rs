//! Dot methods: inventory(owner), give(owner, stack), take(owner, slot, count),
//! transfer_inventory(from, source, to, destination, count),
//! move_slots(owner, from, to, count), collect_drop(entity, max_count).
//! Slots are zero-based;
//! inventory returns a readonly 1-indexed sequence of {stack, insert, extract}.
//! A stack is {item, count, components = nil | {version, bytes}}; bytes is a binary
//! string, never text. Give creates items explicitly; take consumes them. False /
//! nil means no change, not partial fulfillment. Transfer is atomic across owners.
//!
//! Owner is "player" (only the requesting actor) or {entity_lo, entity_hi}, as in
//! an event drop candidate. Entity access/slot permissions/delays remain host
//! decisions. No profile IDs or private entity payloads cross this boundary.
//! Fixed raw field reads never invoke metamethods or traverse arbitrary tables.
//! Context bounds inventories to 256 slots; the live host validates component
//! payloads to 1024 bytes and counts to 128. Decode checks those limits before
//! copying strings. Each operation also consumes the shared Context budget.
use super::bindings::{checked, entity_id, invalid};
use super::{Context, Error, RefCell, Value, integer, text};
use bloxgloom_host_api::gameplay::{Components, InventoryId, Stack};
use mlua::{Lua, Scope, Table};

pub(super) fn install<'scope, 'env: 'scope>(
    scope: &'scope Scope<'scope, 'env>,
    host: &Table,
    context: &'env RefCell<&mut Context<'_>>,
    rejected: &'env RefCell<Option<Error>>,
) -> mlua::Result<()> {
    host.set(
        "inventory",
        scope.create_function(|lua, value: Value| {
            let result = checked(rejected, || {
                let mut context = context.borrow_mut();
                let owner = owner(&context, value)?;
                context.inventory(owner)
            })
            .and_then(|slots| {
                let result = lua.create_table()?;
                for (index, slot) in slots.iter().enumerate() {
                    let value = lua.create_table()?;
                    value.set("insert", slot.insert)?;
                    value.set("extract", slot.extract)?;
                    value.set(
                        "stack",
                        slot.stack
                            .as_ref()
                            .map(|s| stack_table(lua, s))
                            .transpose()?,
                    )?;
                    value.set_readonly(true);
                    result.raw_set(index + 1, value)?;
                }
                result.set_readonly(true);
                Ok(result)
            });
            latch(rejected, result)
        })?,
    )?;
    host.set(
        "give",
        scope.create_function(|_, (value, stack): (Value, Value)| {
            checked(rejected, || {
                let mut context = context.borrow_mut();
                let owner = owner(&context, value)?;
                context.give(owner, decode_stack(stack)?)
            })
        })?,
    )?;
    host.set(
        "take",
        scope.create_function(|lua, (value, slot, count): (Value, Value, Value)| {
            let result = checked(rejected, || {
                let mut context = context.borrow_mut();
                let owner = owner(&context, value)?;
                context.take(owner, index(slot)?, amount(count)?)
            })
            .and_then(|stack| stack.as_ref().map(|s| stack_table(lua, s)).transpose());
            latch(rejected, result)
        })?,
    )?;
    host.set(
        "transfer_inventory",
        scope.create_function(
            |_, (from, source, to, destination, count): (Value, Value, Value, Value, Value)| {
                checked(rejected, || {
                    let mut context = context.borrow_mut();
                    let from = owner(&context, from)?;
                    let to = owner(&context, to)?;
                    context.transfer(
                        from,
                        index(source)?,
                        to,
                        index(destination)?,
                        amount(count)?,
                    )
                })
            },
        )?,
    )?;
    host.set(
        "move_slots",
        scope.create_function(
            |_, (value, from, to, count): (Value, Value, Value, Value)| {
                checked(rejected, || {
                    let mut context = context.borrow_mut();
                    let owner = owner(&context, value)?;
                    context.move_slots(owner, index(from)?, index(to)?, amount(count)?)
                })
            },
        )?,
    )?;
    host.set(
        "collect_drop",
        scope.create_function(|_, (value, count): (Value, Value)| {
            checked(rejected, || {
                let mut context = context.borrow_mut();
                let InventoryId::Entity(id) = owner(&context, value)? else {
                    return Err(invalid("collect_drop requires an entity owner"));
                };
                context.collect_drop(id, amount(count)?)
            })
        })?,
    )?;
    // All captures are scoped borrows; no VM identity survives dispatch/retry.
    Ok(())
}

fn owner(context: &Context<'_>, value: Value) -> Result<InventoryId, Error> {
    match value {
        Value::String(s) if s.as_bytes().as_ref() == b"player" => context
            .player()
            .ok_or_else(|| invalid("event has no player inventory")),
        Value::Table(t) => Ok(InventoryId::Entity(entity_id(
            field(&t, "entity_lo")?,
            field(&t, "entity_hi")?,
        )?)),
        _ => Err(invalid(
            "inventory owner must be player or entity ID halves",
        )),
    }
}

fn field(table: &Table, key: &str) -> Result<Value, Error> {
    table.raw_get(key).map_err(|e| invalid(&e.to_string()))
}

fn index(value: Value) -> Result<usize, Error> {
    integer(value, 0, 255).map(|n| n as usize).map_err(invalid)
}

fn amount(value: Value) -> Result<u16, Error> {
    integer(value, 1, 128).map(|n| n as u16).map_err(invalid)
}

pub(super) fn decode_stack(value: Value) -> Result<Stack, Error> {
    let Value::Table(table) = value else {
        return Err(invalid("expected stack table"));
    };
    let item = text(field(&table, "item")?).map_err(invalid)?;
    let count = amount(field(&table, "count")?)?;
    let components = match field(&table, "components")? {
        Value::Nil => None,
        Value::Table(table) => {
            let version =
                integer(field(&table, "version")?, 1, u16::MAX.into()).map_err(invalid)? as u16;
            let Value::String(bytes) = field(&table, "bytes")? else {
                return Err(invalid("expected binary component string"));
            };
            if bytes.as_bytes().is_empty()
                || bytes.as_bytes().len() > crate::inventory::MAX_COMPONENT_BYTES
            {
                return Err(invalid("component byte limit exceeded"));
            }
            Some(Components {
                version,
                bytes: bytes.as_bytes().to_vec(),
            })
        }
        _ => return Err(invalid("expected components table or nil")),
    };
    Ok(Stack {
        item,
        count,
        components,
    })
}

fn stack_table(lua: &Lua, stack: &Stack) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("item", stack.item.as_str())?;
    table.set("count", stack.count)?;
    if let Some(components) = &stack.components {
        let value = lua.create_table()?;
        value.set("version", components.version)?;
        value.set("bytes", lua.create_string(&components.bytes)?)?;
        value.set_readonly(true);
        table.set("components", value)?;
    }
    table.set_readonly(true);
    Ok(table)
}

fn latch<T>(rejected: &RefCell<Option<Error>>, result: mlua::Result<T>) -> mlua::Result<T> {
    // VM allocation failures must also poison a staged take even inside pcall.
    result.inspect_err(|error| {
        rejected
            .borrow_mut()
            .get_or_insert_with(|| invalid(&error.to_string()));
    })
}
