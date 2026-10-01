use super::*;

fn runtime() -> Lua {
    let lua = Lua::new();
    install(&lua).unwrap();
    lua
}

#[test]
fn protected_calls_preserve_values_and_normal_errors() {
    let lua = runtime();
    lua.load(
        r#"
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
    "#,
    )
    .exec()
    .unwrap();
    assert!(!exceeded(&lua));
}

#[test]
fn protected_calls_can_yield_and_resume_without_a_rust_boundary() {
    let lua = runtime();
    lua.load(
        r#"
        for _, protected in {pcall, function(fn) return xpcall(fn, function(e) return e end) end} do
            local co=coroutine.create(function()
                local ok,value=protected(function() coroutine.yield(1); return 3 end)
                assert(ok and value==3)
                return 2
            end)
            local ok,value=coroutine.resume(co); assert(ok and value==1)
            local ok,value=coroutine.resume(co); assert(ok and value==2)
        end
    "#,
    )
    .exec()
    .unwrap();
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
