//! Shared, scoped frozen-content and captured-environment queries.
use super::values::{integer, text};
use bloxgloom_host_api::{content::TagKind, gameplay::Environment, queries::Tags};
use mlua::{Lua, Table, Value};
use std::cell::Cell;

pub(in crate::server::script) fn with<R>(
    lua: &Lua,
    host: &Table,
    environment: Option<Environment>,
    tags: Option<&dyn Tags>,
    operation: impl FnOnce() -> mlua::Result<R>,
) -> mlua::Result<R> {
    let calls = Cell::new(0u16);
    let failed = Cell::new(false);
    let calls = &calls;
    let failed = &failed;
    lua.scope(|scope| {
        host.set_readonly(false);
        host.set(
            "world_time",
            scope.create_function(move |lua, ()| {
                guarded(calls, failed, || {
                    let time = environment
                        .ok_or_else(|| invalid("environment unavailable"))?
                        .world_time;
                    let t = lua.create_table()?;
                    t.set("elapsed_ms", time.elapsed_ms)?;
                    t.set("cycle_ms", time.cycle_ms)?;
                    t.set_readonly(true);
                    Ok(t)
                })
            })?,
        )?;
        host.set(
            "weather",
            scope.create_function(move |lua, ()| {
                guarded(calls, failed, || {
                    crate::weather::luau::present(
                        lua,
                        environment
                            .ok_or_else(|| invalid("environment unavailable"))?
                            .weather,
                    )
                })
            })?,
        )?;
        install_tags(scope, host, tags, calls, failed)?;
        host.set_readonly(true);
        let result = operation();
        if failed.get() {
            Err(invalid("read service failed; callback rejected"))
        } else {
            result
        }
    })
}

pub(in crate::server::script) fn tags_with<R>(
    lua: &Lua,
    host: &Table,
    tags: Option<&dyn Tags>,
    operation: impl FnOnce() -> mlua::Result<R>,
) -> mlua::Result<R> {
    let calls = Cell::new(0u16);
    let failed = Cell::new(false);
    let calls = &calls;
    let failed = &failed;
    lua.scope(|scope| {
        host.set_readonly(false);
        install_tags(scope, host, tags, calls, failed)?;
        host.set_readonly(true);
        let result = operation();
        if failed.get() {
            Err(invalid("tag query failed; callback rejected"))
        } else {
            result
        }
    })
}

fn install_tags<'scope, 'env: 'scope>(
    scope: &'scope mlua::Scope<'scope, 'env>,
    host: &Table,
    tags: Option<&'env dyn Tags>,
    calls: &'env Cell<u16>,
    failed: &'env Cell<bool>,
) -> mlua::Result<()> {
    host.set(
        "tag_contains",
        scope.create_function(move |_, (kind, key, member): (Value, Value, Value)| {
            guarded(calls, failed, || {
                let members = members(tags, kind, key)?;
                let member = text(member).map_err(invalid)?;
                Ok(members.contains(&member))
            })
        })?,
    )?;
    host.set(
        "tag_members",
        scope.create_function(
            move |lua, (kind, key, offset, limit): (Value, Value, Value, Value)| {
                guarded(calls, failed, || {
                    let members = members(tags, kind, key)?;
                    let offset = if offset.is_nil() {
                        0
                    } else {
                        integer(offset, 0, 4096).map_err(invalid)? as usize
                    };
                    let limit = if limit.is_nil() {
                        128
                    } else {
                        integer(limit, 1, 128).map_err(invalid)? as usize
                    };
                    let result =
                        lua.create_sequence_from(members.iter().skip(offset).take(limit).cloned())?;
                    result.set_readonly(true);
                    Ok((result, members.len()))
                })
            },
        )?,
    )?;
    Ok(())
}

fn members(
    tags: Option<&dyn Tags>,
    kind: Value,
    key: Value,
) -> mlua::Result<&std::collections::BTreeSet<String>> {
    let kind = match text(kind).map_err(invalid)?.as_str() {
        "block" => TagKind::Block,
        "item" => TagKind::Item,
        _ => return Err(invalid("tag kind must be block or item")),
    };
    let key = text(key).map_err(invalid)?;
    tags.and_then(|tags| tags.members(kind, &key))
        .ok_or_else(|| invalid("unknown tag or unavailable catalog"))
}
fn guarded<T>(
    calls: &Cell<u16>,
    failed: &Cell<bool>,
    operation: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let result = if failed.get() || calls.get() >= 64 {
        Err(invalid("read service operation limit exceeded"))
    } else {
        calls.set(calls.get() + 1);
        operation()
    };
    if result.is_err() {
        failed.set(true);
    }
    result
}
fn invalid(message: &str) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}

#[cfg(test)]
mod tests;
