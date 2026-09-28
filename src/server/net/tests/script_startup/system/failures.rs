use super::*;

#[test]
fn luau_system_registration_and_persisted_identity_fail_closed() {
    let fixture = Fixture::new();
    for register in [
        REGISTER.replace("key='demo:clock'", "key='other:clock'"),
        REGISTER.replace("schema=1", "schema=0"),
        REGISTER.replace("revision=1", "revision=1.5"),
        REGISTER.replace("module='demo:clock'", "module='demo:missing'"),
        REGISTER.replace("max_state_bytes=64", "max_state_bytes=4097"),
        REGISTER.replace("max_jobs_per_tick=2", "max_jobs_per_tick=9"),
        REGISTER.replace("read_world=true", "read_world=1"),
        REGISTER.replace("data=string.char(0,255)", "data=string.rep('x',65)"),
        REGISTER.replace("y=5", "y=0.5"),
        REGISTER.replace("seeds={{", "seeds={[2]={"),
        REGISTER.replace("seeds={{", "seeds={hidden={"),
        REGISTER.replace("seeds={{x=0,y=5,z=0,data=string.char(0,255)}}", "seeds=table.create(33,{x=0,y=5,z=0,data=''})"),
        "return function(h) pcall(function() h.register_system({}) end) end".into(),
        REGISTER.replace("h.register_system", "local function register(d) h.register_system(d); pcall(function() h.register_system(d) end) end; register"),
    ] {
        fixture.system(&register, SOURCE);
        assert!(fixture.open().is_err(), "accepted {register}");
        assert!(!fixture.0.join("save").exists());
    }
    fixture.package("demo", "module clock clock.luau", REGISTER);
    assert!(fixture.open().is_err(), "undeclared capability accepted");
    assert!(!fixture.0.join("save").exists());

    fixture.package("helper", "", "return function(_) end");
    fixture.system(REGISTER, SOURCE);
    drop(fixture.open().unwrap());
    let files = ["content.map", "server.wal"];
    let original = files.map(|file| std::fs::read(fixture.0.join("save").join(file)).unwrap());
    for (register, source) in [
        (REGISTER.to_owned(), format!("{SOURCE}\n-- changed module")),
        (REGISTER.replace("schema=1", "schema=2"), SOURCE.into()),
        (REGISTER.replace("revision=1", "revision=2"), SOURCE.into()),
    ] {
        fixture.system(&register, &source);
        assert!(
            fixture.open().is_err(),
            "changed system identity reopened save"
        );
        for (file, bytes) in files.iter().zip(&original) {
            assert_eq!(
                &std::fs::read(fixture.0.join("save").join(file)).unwrap(),
                bytes
            );
        }
    }
    fixture.system(REGISTER, SOURCE);
    fixture.package(
        "helper",
        "",
        "return function(_) end -- changed installation",
    );
    assert!(fixture.open().is_err());
    fixture.package("helper", "", "return function(_) end");
    drop(fixture.open().unwrap());
}

#[test]
fn luau_system_worker_faults_and_caught_host_errors_never_commit_partial_output() {
    for body in [
        "error('failed after proposal')",
        "pcall(function() c.block(2.5,80,0) end)",
        "pcall(function() c.block(16,80,0) end)",
        "pcall(function() for i=1,65 do c.block(2,80,0) end end)",
        "pcall(function() for i=1,17 do c.edit(3,80,0,'bloxgloom:air','bloxgloom:stone') end end)",
        "pcall(function() for i=1,33 do c.wake('demo:clock',0,5,0) end end)",
        "c.edit(3,80,0,'bloxgloom:stone','bloxgloom:glowstone')",
        "c.edit(16,80,0,'bloxgloom:air','bloxgloom:stone')",
        "c.wake('unknown:clock',0,5,0)",
        "pcall(function() while true do end end)",
        "local _ = string.rep('x',16000000)",
        "return string.rep('x',65),1",
        "return 'bad',0",
        "return 'bad',1.5",
    ] {
        let fixture = Fixture::new();
        fixture.system(
            REGISTER,
            &format!(
                r#"return function(c)
            c.edit(2,80,0,c.block(2,80,0),'bloxgloom:glowstone')
            c.wake('demo:clock',0,5,0)
            do {body} end
            return 'partial',1
        end"#
            ),
        );
        let mut state = fixture.open().unwrap();
        state.world.get_chunk(KEY).unwrap();
        let wal = std::fs::read(fixture.0.join("save/server.wal")).unwrap();
        assert!(
            stage(&mut state, "demo:clock", 1).is_err(),
            "accepted {body}"
        );
        assert_eq!(value(&state, "demo:clock"), (0, vec![0, 255]), "{body}");
        assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR), "{body}");
        assert_eq!(
            std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
            wal,
            "{body}"
        );
        drop(state);
        assert_eq!(
            value(&fixture.open().unwrap(), "demo:clock"),
            (0, vec![0, 255])
        );
    }
}

#[test]
fn luau_system_multi_owner_wave_is_fresh_and_atomic_on_worker_failure() {
    for fail in [false, true] {
        let fixture = Fixture::new();
        fixture.system(
            &REGISTER
                .replace("read_world=true", "read_world=false")
                .replace(
                    "seeds={{x=0,y=5,z=0,data=string.char(0,255)}}",
                    "seeds={{x=0,y=5,z=0,data=string.char(0)}, {x=1,y=5,z=0,data=string.char(0)}}",
                ),
            &format!(
                r#"
                local calls = 0
                return function(c)
                    calls += 1; assert(calls == 1)
                    assert(c.tick_hi == 1 and c.tick_lo >= 7)
                    assert(c.revision_hi == 0)
                    assert(string.byte(c.data) == c.revision_lo)
                    if {fail} and c.owner[1] == 1 then error('second owner failed') end
                    return string.char(string.byte(c.data)+1), 2
                end
            "#
            ),
        );
        let mut state = fixture.open().unwrap();
        let tick = (1 << 32) + 7;
        if fail {
            let error = stage(&mut state, "demo:clock", tick).err().unwrap();
            assert!(error.to_string().contains("demo@1.0.0:clock"), "{error}");
        } else {
            commit(&mut state, "demo:clock", tick);
            commit(&mut state, "demo:clock", tick + 2);
        }
        for x in 0..2 {
            let (revision, data) = state
                .system_runtime
                .owner_value::<Vec<u8>>(
                    &SystemId::new("demo:clock").unwrap(),
                    OwnerKey::Chunk(ChunkKey { x, ..KEY }),
                )
                .unwrap();
            let expected = if fail { 0 } else { 2 };
            assert_eq!(revision, expected);
            assert_eq!(data, [expected as u8]);
        }
    }
}
