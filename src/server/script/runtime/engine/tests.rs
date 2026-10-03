use super::*;

#[test]
fn package_reload_resets_retained_imports_and_keeps_in_flight_callback_revision() {
    use crate::server::script::package::PackageSnapshot;
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-retained-reload-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("demo")).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    std::fs::write(root.join("demo/package.txt"), "format 1\npackage demo\nversion 1.0.0\nentry main\nmodule main main.luau\nmodule helper helper.luau\n").unwrap();
    std::fs::write(
        root.join("demo/main.luau"),
        "local n=0; return function() n+=1; return import('demo:helper')+n end",
    )
    .unwrap();
    std::fs::write(root.join("demo/helper.luau"), "return 10").unwrap();
    let original = Arc::new(PackageSnapshot::discover(&root).unwrap());
    let program = Program::Package {
        snapshot: Arc::clone(&original),
        entry: "demo:main".into(),
        invocation: crate::server::script::Invocation::Integer,
    };
    let mut realm = Retained::default();
    let call = |realm: &mut Retained| {
        realm.run(
            &program,
            Limits::default(),
            Execution::new("test", 1, "reload"),
            |_, entry| entry.call::<i64>(()),
        )
    };
    assert_eq!(call(&mut realm).unwrap(), 11);
    assert_eq!(call(&mut realm).unwrap(), 12);
    std::fs::write(root.join("demo/helper.luau"), "return 100").unwrap();
    let replacement = original.replacement(&root).unwrap();
    // Publish after execution selected its snapshot, before the script's first
    // helper import. The in-flight callback must still import the old helper.
    let value = realm
        .run(
            &program,
            Limits::default(),
            Execution::new("test", 1, "reload"),
            |_, entry| {
                original.publish(Arc::clone(&replacement));
                entry.call::<i64>(())
            },
        )
        .unwrap();
    assert_eq!(value, 13);
    assert_eq!(call(&mut realm).unwrap(), 101);
    assert_eq!(call(&mut realm).unwrap(), 102);
    let retired = Arc::downgrade(&replacement);
    assert!(
        realm
            .run(
                &program,
                Limits::default(),
                Execution::new("test", 1, "reload"),
                |_, _| Err::<(), _>(mlua::Error::RuntimeError("callback failed".into()))
            )
            .is_err()
    );
    let latest = original.replacement(&root).unwrap();
    original.publish(Arc::clone(&latest));
    drop(replacement);
    assert!(
        retired.upgrade().is_none(),
        "failed retained realms must not pin retired source bytes"
    );
    let weak = Arc::downgrade(&original);
    drop(realm);
    drop(program);
    drop(original);
    drop(latest);
    assert!(
        weak.upgrade().is_none(),
        "revision bank must not form an Arc cycle"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn retained_locals_and_coroutines_live_until_an_error_reset() {
    let mut realm = Retained::default();
    let module = SourceModule { id: "test:retained".into(), source: "local n=0; local co=coroutine.create(function() while true do n+=1; coroutine.yield(n) end end); return function(fail) local ok,value=coroutine.resume(co); assert(ok); if fail then error('reset') end; return value end".into() };
    let mut call = |fail| {
        realm.run_source(
            &module,
            Limits::default(),
            Execution::new("test", 7, "test"),
            |_, entry| entry.call::<i64>(fail),
        )
    };
    assert_eq!(call(false).unwrap(), 1);
    assert_eq!(call(false).unwrap(), 2);
    assert!(call(true).is_err());
    assert_eq!(call(false).unwrap(), 1);
}
#[test]
fn reused_attempts_do_not_retain_globals_or_closures_or_rng_draws() {
    let program = Program::Source(SourceModule { id: "test:isolation".into(), source: "assert(leaked == nil); leaked = true; local n=0; return function() n+=1; return n*1000000 + math.random(1,999999) end".into() });
    let call = || {
        isolated(
            &program,
            Limits::default(),
            Execution::new("test", 99, "retry"),
            |_, entry| entry.call::<i64>(()),
        )
    };
    let first = call().unwrap();
    for _ in 0..8 {
        assert_eq!(call().unwrap(), first);
    }
}

#[test]
fn retained_saved_scoped_context_is_revoked_and_coroutine_limits_reset() {
    let module = SourceModule { id: "test:contexts".into(), source: "local old; return function(ctx) if old then assert(not pcall(old)) end; old=ctx; return 1 end".into() };
    let mut realm = Retained::default();
    for _ in 0..2 {
        let value = realm
            .run_source(
                &module,
                Limits::default(),
                Execution::new("test", 0, "test"),
                |lua, entry| {
                    lua.scope(|scope| entry.call::<i64>(scope.create_function(|_, ()| Ok(42))?))
                },
            )
            .unwrap();
        assert_eq!(value, 1);
    }
    let bad = SourceModule { id: "test:bad".into(), source: "local co=coroutine.create(function() while true do end end); return function() coroutine.resume(co); return 1 end".into() };
    assert!(matches!(
        realm
            .run_source(
                &bad,
                Limits {
                    max_interrupts: 10,
                    ..Limits::default()
                },
                Execution::new("test", 0, "test"),
                |_, entry| entry.call::<i64>(())
            )
            .unwrap_err()
            .failure,
        ScriptFailure::InstructionLimit
    ));
    assert_eq!(
        realm
            .run_source(
                &module,
                Limits::default(),
                Execution::new("test", 0, "test"),
                |lua, entry| lua
                    .scope(|scope| entry.call::<i64>(scope.create_function(|_, ()| Ok(42))?))
            )
            .unwrap(),
        1
    );
}
#[test]
fn retained_globals_cannot_replace_host_reseeding_and_caught_memory_resets_realm() {
    let module = SourceModule { id: "test:shadow".into(), source: "local n=0; math={randomseed=function() error('injected') end}; return function() n+=1; return n end".into() };
    let mut realm = Retained::default();
    for n in 1..=2 {
        assert_eq!(
            realm
                .run_source(
                    &module,
                    Limits::default(),
                    Execution::new("test", n, "test"),
                    |_, entry| entry.call::<i64>(())
                )
                .unwrap(),
            n as i64
        );
    }
    let module = SourceModule { id: "test:memory".into(), source: "local n=0; return function(fail) n+=1; if fail then xpcall(function() buffer.create(16*1024*1024) end,function(e) return 'hidden' end) end; return n end".into() };
    assert!(
        realm
            .run_source(
                &module,
                Limits::default(),
                Execution::new("test", 0, "test"),
                |_, entry| entry.call::<i64>(true)
            )
            .is_err()
    );
    assert_eq!(
        realm
            .run_source(
                &module,
                Limits::default(),
                Execution::new("test", 0, "test"),
                |_, entry| entry.call::<i64>(false)
            )
            .unwrap(),
        1
    );
}
