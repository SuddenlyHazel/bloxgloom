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
mod tests {
    use super::*;

    fn runtime() -> Lua {
        let lua = Lua::new();
        install(&lua).unwrap();
        lua
    }

    #[test]
    fn protected_calls_preserve_values_and_normal_errors() {
        let lua = runtime();
        lua.load(r#"
            local ok,a,b,c=pcall(function(x) return x,nil,3 end,1)
            assert(ok and a==1 and b==nil and c==3)
            local ok,err=pcall(function() error('ordinary') end)
            assert(not ok and string.find(err,'ordinary'))
            local ok,err=xpcall(function() error('ordinary') end,function(e) return 'transformed' end)
            assert(not ok and err=='transformed')
            local co=coroutine.create(function(x) coroutine.yield(x,nil,3); return 4 end)
            local ok,a,b,c=coroutine.resume(co,1)
            assert(ok and a==1 and b==nil and c==3)
            local ok,n=coroutine.resume(co); assert(ok and n==4)
        "#).exec().unwrap();
        assert!(!exceeded(&lua));
    }

    #[test]
    fn protected_calls_can_yield_and_resume_without_a_rust_boundary() {
        let lua = runtime();
        lua.load(r#"
            for _, protected in {pcall, function(fn) return xpcall(fn, function(e) return e end) end} do
                local co=coroutine.create(function()
                    local ok,value=protected(function() coroutine.yield(1); return 3 end)
                    assert(ok and value==3)
                    return 2
                end)
                local ok,value=coroutine.resume(co); assert(ok and value==1)
                local ok,value=coroutine.resume(co); assert(ok and value==2)
            end
        "#).exec().unwrap();
        assert!(!exceeded(&lua));
    }

    #[test]
    fn memory_errors_latch_before_handlers_transform_them_and_reset_explicitly() {
        let lua = runtime();
        for source in [
            "local ok,e=pcall(function() error('not enough memory') end); assert(not ok)",
            "local ok,e=xpcall(function() error('not enough memory') end,function(e) return 'hidden' end); assert(not ok and e=='hidden')",
            "local co=coroutine.create(function() error('not enough memory') end); local ok,e=coroutine.resume(co); assert(not ok)",
        ] {
            begin(&lua);
            assert!(!exceeded(&lua));
            lua.load(source).exec().unwrap();
            assert!(exceeded(&lua), "{source}");
        }
        begin(&lua);
        assert!(!exceeded(&lua));
    }

    #[test]
    fn real_allocator_error_caught_in_pcall_is_latched() {
        let lua = runtime();
        let callback: Function = lua.load("return function() local ok=pcall(function() buffer.create(1024*1024) end); return ok end").eval().unwrap();
        lua.set_memory_limit(lua.used_memory() + 128 * 1024)
            .unwrap();
        assert!(!callback.call::<bool>(()).unwrap());
        assert!(exceeded(&lua));
    }
}
