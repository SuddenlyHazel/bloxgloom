//! Exact Luau entity/profile owner identities through the public worker/WAL path.
use super::*;

#[test]
fn entity_and_profile_owner_partitions_wake_commit_join_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    for (kind, words, wake, owner) in [
        (
            "entity",
            "2882400001,305419896",
            "c.wake_entity('demo:clock',c.owner[1],c.owner[2])",
            OwnerKey::Entity(0x1234_5678_abcd_ef01),
        ),
        (
            "profile",
            "1,2,3,4",
            "c.wake_profile('demo:clock',c.owner[1],c.owner[2],c.owner[3],c.owner[4])",
            OwnerKey::Profile(1 | (2 << 32) | (3 << 64) | (4 << 96)),
        ),
    ] {
        let fixture = Fixture::new();
        let register = format!(
            "return function(h) h.register_system{{key='demo:clock',schema=1,revision=1,module='demo:clock',partition='{kind}',max_state_bytes=8,max_jobs_per_tick=1,seeds={{{{id={{{words}}},data='new'}}}}}} end"
        );
        let source = format!(
            "return function(c) assert(c.owner_kind == '{kind}'); if c.data == 'new' then {wake}; return 'done',10000 end assert(c.data == 'done'); return 'woken',10000 end"
        );
        fixture.system(&register, &source);
        let mut state = Box::new(fixture.open().unwrap());
        let id = SystemId::new("demo:clock").unwrap();
        assert_eq!(
            state
                .system_runtime
                .owner_value::<Vec<u8>>(&id, owner)
                .unwrap(),
            (0, b"new".to_vec())
        );
        commit(&mut state, "demo:clock", 1);
        assert_eq!(
            state
                .system_runtime
                .owner_value::<Vec<u8>>(&id, owner)
                .unwrap(),
            (1, b"done".to_vec())
        );
        commit(&mut state, "demo:clock", 2);
        assert_eq!(
            state
                .system_runtime
                .owner_value::<Vec<u8>>(&id, owner)
                .unwrap(),
            (2, b"woken".to_vec())
        );
        let fingerprint = state.world.catalog().fingerprint();
        super::super::gameplay::serve(state, |address| {
            let client = crate::client::connect_catalog_probe(&address.to_string(), 0x537).unwrap();
            assert_eq!(client.fingerprint(), fingerprint);
        });
        let restored = fixture.open().unwrap();
        assert_eq!(restored.world.catalog().fingerprint(), fingerprint);
        assert_eq!(
            restored
                .system_runtime
                .owner_value::<Vec<u8>>(&id, owner)
                .unwrap(),
            (2, b"woken".to_vec())
        );
    }
}

#[test]
fn nonchunk_owner_rejects_world_reads_and_noncanonical_ids_before_save() {
    for (partition, extra, seed) in [
        ("entity", ",read_world=true", "{id={1,0},data='x'}"),
        ("entity", "", "{id={1.5,0},data='x'}"),
        ("entity", "", "{id={[2]=1},data='x'}"),
        ("profile", "", "{id={1,2,3},data='x'}"),
    ] {
        let fixture = Fixture::new();
        fixture.system(
            &format!("return function(h) h.register_system{{key='demo:clock',schema=1,revision=1,module='demo:clock',partition='{partition}',max_state_bytes=8,max_jobs_per_tick=1{extra},seeds={{{seed}}}}} end"),
            "return function(c) return c.data,10000 end",
        );
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/content.map").exists());
    }
}

#[test]
fn caught_invalid_profile_wake_rejects_the_entire_owner_plan() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',partition='profile',max_state_bytes=8,max_jobs_per_tick=1,seeds={{id={1,2,3,4},data='new'}}} end",
        "return function(c) assert(not pcall(function() c.wake_profile('demo:clock',1.5,2,3,4) end)); return 'done',10000 end",
    );
    let mut state = fixture.open().unwrap();
    assert!(stage(&mut state, "demo:clock", 1).is_err());
    assert_eq!(
        state
            .system_runtime
            .owner_value::<Vec<u8>>(
                &SystemId::new("demo:clock").unwrap(),
                OwnerKey::Profile(1 | (2 << 32) | (3 << 64) | (4 << 96)),
            )
            .unwrap(),
        (0, b"new".to_vec())
    );
    drop(state);
    assert!(fixture.open().is_ok());
}
