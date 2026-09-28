use super::*;

#[test]
fn luau_neighborhood_radius_requires_exact_bounds_and_world_capability() {
    for radius in ["-1", "2", "0.5", "true", "'1'", "{}", "0/0", "math.huge"] {
        let fixture = fixture();
        std::fs::write(
            fixture.0.join("packages/relay/main.luau"),
            MAIN.replace(
                "read_radius_chunks = 1",
                &format!("read_radius_chunks = {radius}"),
            ),
        )
        .unwrap();
        assert!(fixture.open().is_err(), "accepted radius {radius}");
        assert!(!fixture.0.join("save").exists());
    }
    for radius in [0, 1] {
        for read in ["false", "nil"] {
            let fixture = fixture();
            std::fs::write(
                fixture.0.join("packages/relay/main.luau"),
                MAIN.replace("read_world = true", &format!("read_world = {read}"))
                    .replace("accepts_intents = true, intent_bootstrap = 'new',", "")
                    .replace(
                        "read_radius_chunks = 1",
                        &format!("read_radius_chunks = {radius}"),
                    ),
            )
            .unwrap();
            assert!(fixture.open().is_err());
            assert!(!fixture.0.join("save").exists());
        }
    }
    let fixture = fixture();
    std::fs::write(
        fixture.0.join("packages/relay/package.txt"),
        MANIFEST.replace("requires bloxgloom:owner_systems/v1", ""),
    )
    .unwrap();
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save").exists());
}

#[test]
fn luau_neighborhood_caught_overreach_and_preimage_errors_poison_every_effect() {
    for (radius, body) in [
        (0, "c.block(18,80,2)"),
        (0, "c.edit(18,80,2,'bloxgloom:air','bloxgloom:stone')"),
        (1, "c.block(34,80,2)"),
        (1, "c.edit(34,80,2,'bloxgloom:air','bloxgloom:stone')"),
        (1, "c.block(2,112,2)"),
        (1, "c.block(2,80,-17)"),
        (1, "c.edit(18,80,2,'bloxgloom:stone','bloxgloom:glowstone')"),
        (1, "for i=1,65 do c.block(18,80,2) end"),
        (
            1,
            "for i=1,16 do c.edit(16+i,80,2,'bloxgloom:air','bloxgloom:stone') end",
        ),
    ] {
        let fixture = fixture();
        std::fs::write(
            fixture.0.join("packages/relay/main.luau"),
            MAIN.replace(
                "read_radius_chunks = 1",
                &format!("read_radius_chunks = {radius}"),
            ),
        )
        .unwrap();
        std::fs::write(
            fixture.0.join("packages/relay/grow.luau"),
            format!(
                r#"
            return function(c)
                c.send(1,5,0,'partial')
                c.edit(2,80,2,'bloxgloom:air','bloxgloom:glowstone')
                c.wake('relay:grow',20,5,0)
                pcall(function() {body} end)
                return 'partial',1
            end"#
            ),
        )
        .unwrap();
        let mut state = fixture.open().unwrap();
        // Even resident chunks outside this job's capture grant no authority.
        load_neighborhoods(&mut state, None);
        let wal = std::fs::read(fixture.0.join("save/server.wal")).unwrap();
        assert!(
            stage(&mut state, SYSTEM, 1).is_err(),
            "accepted {radius}: {body}"
        );
        assert_eq!(state_value(&state, 0), Some((0, b"seed".to_vec())));
        assert!(state_value(&state, 1).is_none());
        assert!(inbox(&state, 1).is_empty());
        assert_eq!(state.world.cached_block(2, 80, 2), Some(AIR));
        assert_eq!(
            std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
            wal
        );
    }
}

#[test]
fn luau_neighborhood_multi_owner_edits_are_atomic_and_reject_overlapping_writes() {
    for overlapping in [false, true] {
        let fixture = fixture();
        std::fs::write(
            fixture.0.join("packages/relay/main.luau"),
            MAIN.replace(
                "seeds = {{x=0, y=5, z=0, data='seed'}}",
                "seeds = {{x=0,y=5,z=0,data='seed'}, {x=1,y=5,z=0,data='seed'}}",
            ),
        )
        .unwrap();
        std::fs::write(
            fixture.0.join("packages/relay/grow.luau"),
            format!(
                r#"
            return function(c)
                local x = if {overlapping} then 18 else (c.owner[1]+1)*16+2
                c.edit(x,80,2,c.block(x,80,2),'bloxgloom:glowstone')
                return 'done',100000
            end"#
            ),
        )
        .unwrap();
        let mut state = fixture.open().unwrap();
        load_neighborhoods(&mut state, None);
        let wal = std::fs::read(fixture.0.join("save/server.wal")).unwrap();
        if overlapping {
            assert!(stage(&mut state, SYSTEM, 1).is_err());
            assert_eq!(
                std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
                wal
            );
        } else {
            commit(&mut state, SYSTEM, 1);
        }
        drop(state);
        let mut state = fixture.open().unwrap();
        for x in 0..=1 {
            assert_eq!(
                state_value(&state, x),
                Some(if overlapping {
                    (0, b"seed".to_vec())
                } else {
                    (1, b"done".to_vec())
                })
            );
            assert_eq!(
                state.world.get_block((x + 1) * 16 + 2, 80, 2).unwrap(),
                if overlapping { AIR } else { GLOWSTONE }
            );
        }
    }
}
