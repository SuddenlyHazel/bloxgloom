//! Latch allocator rejections even when Luau protected calls consume the error.
use mlua::{Function, Lua, Value};
use std::cell::Cell;

struct Rejected(Cell<bool>);

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_app_data(Rejected(Cell::new(false)));
    let globals = lua.globals();
    let table: mlua::Table = globals.get("table")?;
    let pack: Function = table.get("pack")?;
    let unpack: Function = table.get("unpack")?;
    let observe = lua.create_function(|lua, (ok, error): (bool, Value)| {
        if !ok {
            observe(lua, &error);
        }
        Ok(())
    })?;
    // Keep protected execution in Lua: invoking the original through a Rust
    // callback would prevent coroutine yields across that C-call boundary.
    let protect: Function = lua
        .load(
            r#"
        return function(original, observe, pack, unpack)
            return function(...)
                local values = pack(original(...))
                observe(values[1], values[2])
                return unpack(values, 1, values.n)
            end
        end
    "#,
        )
        .set_name("runtime:memory-protected-call")
        .eval()?;
    let original: Function = globals.get("pcall")?;
    globals.set(
        "pcall",
        protect.call::<Function>((original, observe.clone(), pack.clone(), unpack.clone()))?,
    )?;
    let guard_xpcall: Function = lua
        .load(
            r#"
        return function(original, observe, pack, unpack, kind)
            return function(callback, handler, ...)
                if kind(handler) ~= 'function' then
                    return original(callback, handler, ...)
                end
                local function guarded(error)
                    observe(false, error)
                    return handler(error)
                end
                local values = pack(original(callback, guarded, ...))
                observe(values[1], values[2])
                return unpack(values, 1, values.n)
            end
        end
    "#,
        )
        .set_name("runtime:memory-xpcall")
        .eval()?;
    let original: Function = globals.get("xpcall")?;
    let kind: Function = globals.get("type")?;
    globals.set(
        "xpcall",
        guard_xpcall.call::<Function>((
            original,
            observe.clone(),
            pack.clone(),
            unpack.clone(),
            kind,
        ))?,
    )?;
    let coroutine: mlua::Table = globals.get("coroutine")?;
    let original: Function = coroutine.get("resume")?;
    coroutine.set(
        "resume",
        protect.call::<Function>((original, observe, pack, unpack))?,
    )?;
    Ok(())
}

pub(crate) fn begin(lua: &Lua) {
    if let Some(rejected) = lua.app_data_ref::<Rejected>() {
        rejected.0.set(false);
    }
}

pub(crate) fn exceeded(lua: &Lua) -> bool {
    lua.app_data_ref::<Rejected>()
        .is_some_and(|rejected| rejected.0.get())
}

fn reject(lua: &Lua) {
    if let Some(rejected) = lua.app_data_ref::<Rejected>() {
        rejected.0.set(true);
    }
}

fn memory_error(error: &mlua::Error) -> bool {
    matches!(error, mlua::Error::MemoryError(_)) || memory_text(error.to_string().as_bytes())
}

fn memory_text(text: &[u8]) -> bool {
    text.windows(b"not enough memory".len())
        .any(|part| part == b"not enough memory")
        || text
            .windows(b"memory allocation error".len())
            .any(|part| part == b"memory allocation error")
}

fn observe(lua: &Lua, error: &Value) {
    let rejected = match error {
        Value::Error(error) => memory_error(error),
        Value::String(text) => memory_text(text.as_bytes().as_ref()),
        _ => false,
    };
    if rejected {
        reject(lua);
    }
}

#[cfg(test)]
mod tests;
