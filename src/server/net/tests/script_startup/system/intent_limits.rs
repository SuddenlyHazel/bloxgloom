use super::*;

#[test]
fn luau_intent_full_inbox_is_immutable_and_failed_consumer_keeps_every_id() {
    for fail in [false, true] {
        let fixture = Fixture::new();
        fixture.system(&registration(), &format!(r#"
            return function(c)
                if c.owner[1] == 0 then
                    for i=1,8 do c.send(1,5,0,string.rep(string.char(i),512)) end
                else
                    assert(#c.inbox == 8)
                    for i,m in c.inbox do
                        assert(m.id.ordinal == i-1 and m.id.revision_lo == 1)
                        assert(m.produced_tick_lo == 1 and m.payload == string.rep(string.char(i),512))
                    end
                    assert(not pcall(function() c.inbox[1] = false end))
                    assert(not pcall(function() c.inbox[1].payload = 'changed' end))
                    assert(not pcall(function() c.inbox[1].id.ordinal = 9 end))
                    assert(not pcall(function() c.inbox[1].id.source[1] = 9 end))
                    c.edit(18,80,0,c.block(18,80,0),'bloxgloom:glowstone')
                    c.send(2,5,0,'forward')
                    if {fail} then
                        pcall(function() c.send(2,5,0,string.rep('x',513)) end)
                    end
                end
                return 'done',100000
            end
        "#));
        let mut state = Box::new(fixture.open().unwrap());
        state.world.get_chunk(KEY).unwrap();
        commit(&mut state, "demo:clock", 1);
        let messages = inbox(&state, 1);
        assert_eq!(messages.len(), 8);
        drop(state);
        let mut state = Box::new(fixture.open().unwrap());
        assert_eq!(inbox(&state, 1), messages);
        state.world.get_chunk(ChunkKey { x: 1, ..KEY }).unwrap();
        if fail {
            let wal = std::fs::read(fixture.0.join("save/server.wal")).unwrap();
            for tick in [2, 3] {
                assert!(stage(&mut state, "demo:clock", tick).is_err());
                assert_eq!(inbox(&state, 1), messages);
                assert_eq!(state_value(&state, 1), Some((0, b"new".to_vec())));
                assert!(state_value(&state, 2).is_none());
                assert_eq!(state.world.cached_block(18, 80, 0), Some(AIR));
            }
            assert_eq!(
                std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
                wal
            );
            drop(state);
            assert_eq!(inbox(&fixture.open().unwrap(), 1), messages);
        } else {
            commit(&mut state, "demo:clock", 2);
            assert!(inbox(&state, 1).is_empty());
            assert_eq!(state_value(&state, 1), Some((1, b"done".to_vec())));
            assert_eq!(inbox(&state, 2).len(), 1);
            assert_eq!(state.world.cached_block(18, 80, 0), Some(GLOWSTONE));
        }
    }
}

#[test]
fn luau_intent_send_requires_opt_in_and_absence_requires_bootstrap() {
    for register in [
        REGISTER.to_owned(),
        registration().replace(", intent_bootstrap='new'", ""),
    ] {
        let fixture = Fixture::new();
        fixture.system(
            &register,
            r#"return function(c)
            pcall(function() c.send(1,5,0,'message') end)
            c.edit(2,80,0,c.block(2,80,0),'bloxgloom:glowstone')
            return 'partial',100000
        end"#,
        );
        let mut state = fixture.open().unwrap();
        state.world.get_chunk(KEY).unwrap();
        let wal = std::fs::read(fixture.0.join("save/server.wal")).unwrap();
        assert!(stage(&mut state, "demo:clock", 1).is_err());
        assert_eq!(state_value(&state, 0), Some((0, vec![0, 255])));
        assert!(state_value(&state, 1).is_none());
        assert!(inbox(&state, 1).is_empty());
        assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
        assert_eq!(
            std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
            wal
        );
    }
}
