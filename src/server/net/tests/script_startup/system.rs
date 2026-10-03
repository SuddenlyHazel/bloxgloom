//! Local package -> public system -> owner worker -> WAL -> real TCP/recovery.
use super::*;
use crate::server::{
    durable::complete_barrier,
    parallel::OwnerKey,
    registry::SystemId,
    runtime::systems::{PendingRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs},
    simulation::TickId,
};
use crate::world::{AIR, ChunkKey, GLOWSTONE, STONE};

#[path = "system/decisions.rs"]
mod decisions;
#[path = "system/failures.rs"]
mod failures;
#[path = "system/intents.rs"]
mod intents;
#[path = "system/neighborhood.rs"]
mod neighborhood;
#[path = "system/partitions.rs"]
mod partitions;

#[test]
fn luau_owner_after_dependencies_are_resolved_before_save_creation() {
    let fixture = Fixture::new();
    fixture.system(REGISTER, SOURCE);
    fixture.package("later", "requires bloxgloom:owner_systems/v1\ndependency demo 1.0.0\nmodule clock clock.luau", "return function(h) h.register_system{key='later:clock',schema=1,revision=1,module='later:clock',max_state_bytes=8,max_jobs_per_tick=1,after={'demo:clock'},seeds={{x=0,y=5,z=0,data='x'}}} end");
    std::fs::write(
        fixture.0.join("packages/later/clock.luau"),
        "return function(c) return c.data, 100000 end",
    )
    .unwrap();
    let state = fixture.open().unwrap();
    assert!(
        state
            .phase_plan
            .system(&SystemId::new("later:clock").unwrap())
            .is_some()
    );
    drop(state);
    let bad = Fixture::new();
    bad.system("return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,after={'demo:missing'},seeds={{x=0,y=5,z=0,data='x'}}} end", SOURCE);
    assert!(bad.open().is_err());
    assert!(!bad.0.join("save/content.map").exists());
}

#[test]
fn luau_system_schema_identity_survives_behavior_edits() {
    let fixture = Fixture::new();
    fixture.system(REGISTER, SOURCE);
    let state = fixture.open().unwrap();
    let manifest = crate::content::ContentManifest::from_catalog(state.world.catalog());
    let expected = manifest
        .entries
        .iter()
        .find(|e| e.key == "demo:clock" && e.kind == b'Y')
        .unwrap()
        .schema_fingerprint;
    drop(state);
    fixture.system(REGISTER, "return function(c) return c.data, 200000 end");
    let state = fixture.open().unwrap();
    let manifest = crate::content::ContentManifest::from_catalog(state.world.catalog());
    assert_eq!(
        manifest
            .entries
            .iter()
            .find(|e| e.key == "demo:clock" && e.kind == b'Y')
            .unwrap()
            .schema_fingerprint,
        expected
    );
}

const KEY: ChunkKey = ChunkKey { x: 0, y: 5, z: 0 };

#[test]
fn luau_burn_owner_uses_host_removal_semantics_and_persists_receipt() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,edit_cause='burn',seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) if c.data == 'new' then local old=c.block(2,80,0); assert(old == 'bloxgloom:stone'); c.edit(2,80,0,old,'bloxgloom:air') end return 'done',10000 end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.world.edit(2, 80, 0, STONE).unwrap();
    state.world.get_chunk(KEY).unwrap();
    commit(&mut state, "demo:clock", 1);
    assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
    assert_eq!(value(&state, "demo:clock"), (1, b"done".to_vec()));
    let fingerprint = state.world.catalog().fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x530).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
    });
    let mut restored = fixture.open().unwrap();
    assert_eq!(restored.world.get_block(2, 80, 0).unwrap(), AIR);
    assert_eq!(value(&restored, "demo:clock"), (1, b"done".to_vec()));
    drop(restored);
    let invalid = Fixture::new();
    invalid.system("return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,edit_cause='burn',seeds={{x=0,y=5,z=0,data='x'}}} end", SOURCE);
    assert!(invalid.open().is_err());
    assert!(!invalid.0.join("save/content.map").exists());
}

#[test]
fn luau_owner_block_info_uses_captured_public_fields_and_restarts() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) local block=c.block_info(2,80,0); assert(block.state=='bloxgloom:stone' and block.block_type=='bloxgloom:stone' and block.primary_item=='bloxgloom:stone'); assert(type(block.plant)=='boolean' and type(block.supports_plant)=='boolean'); assert(not pcall(function() block.state='forged' end)); assert(c.block(2,80,0)==block.state); c.edit(2,80,0,block.state,'bloxgloom:glowstone'); return 'done',10000 end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.world.edit(2, 80, 0, STONE).unwrap();
    state.world.get_chunk(KEY).unwrap();
    commit(&mut state, "demo:clock", 1);
    assert_eq!(state.world.cached_block(2, 80, 0), Some(GLOWSTONE));
    let fingerprint = state.world.catalog().fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x548).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
    });
    let restored = fixture.open().unwrap();
    assert_eq!(restored.world.catalog().fingerprint(), fingerprint);
    assert_eq!(value(&restored, "demo:clock"), (1, b"done".to_vec()));
    assert_eq!(restored.world.cached_block(2, 80, 0), Some(GLOWSTONE));

    let bad = Fixture::new();
    bad.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) pcall(function() c.block_info(1600,80,0) end); return 'bad',10000 end",
    );
    let mut state = bad.open().unwrap();
    state.world.get_chunk(KEY).unwrap();
    assert!(stage(&mut state, "demo:clock", 1).is_err());
    assert_eq!(value(&state, "demo:clock"), (0, b"new".to_vec()));
}

#[test]
fn luau_owner_drop_creation_shares_receipt_and_restarts() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,creates_drops=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) if c.data == 'new' then c.spawn_drop(2,80,0,'bloxgloom:stick',2,0) end return 'done',10000 end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    state.world.get_chunk(KEY).unwrap();
    let (wave, missing) = stage(&mut state, "demo:clock", 1).unwrap();
    assert!(missing.is_empty());
    let position = [2.5, 80.5, 0.5];
    assert!(crate::server::drops::nearby(&state.entities, position).is_empty());
    complete_barrier(&mut state, wave.unwrap().barrier()).unwrap();
    assert_eq!(value(&state, "demo:clock"), (1, b"done".to_vec()));
    assert_eq!(
        crate::server::drops::nearby(&state.entities, position)[0].count,
        2
    );
    let fingerprint = state.world.catalog().fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x531).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
    });
    let restored = fixture.open().unwrap();
    assert_eq!(value(&restored, "demo:clock"), (1, b"done".to_vec()));
    assert_eq!(
        crate::server::drops::nearby(&restored.entities, position)[0].count,
        2
    );
}

#[test]
fn luau_owner_drop_errors_poison_caught_plan() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,creates_drops=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) pcall(function() c.spawn_drop(2,80,0,'missing:item',1,0) end); return 'done',10000 end",
    );
    let mut state = fixture.open().unwrap();
    state.world.get_chunk(KEY).unwrap();
    assert!(stage(&mut state, "demo:clock", 1).is_err());
    assert_eq!(value(&state, "demo:clock"), (0, b"new".to_vec()));
    assert!(crate::server::drops::nearby(&state.entities, [2.5, 80.5, 0.5]).is_empty());
    drop(state);
    let invalid = Fixture::new();
    invalid.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,creates_drops=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        SOURCE,
    );
    assert!(invalid.open().is_err());
    assert!(!invalid.0.join("save/content.map").exists());
}

#[test]
fn luau_owner_general_entity_spawn_shares_receipt_and_restarts() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_entity('demo:marker',1,1,1,nil); h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,creates_drops=true,creates_entities=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) if c.data == 'new' then c.spawn_entity('demo:marker',2,80,0,'x'); c.spawn_drop(3,80,0,'bloxgloom:stick',2,0); c.edit(4,80,0,c.block(4,80,0),'bloxgloom:glowstone') end return 'done',10000 end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}\nrequires bloxgloom:actions/v1\n"),
    )
    .unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    state.world.get_chunk(KEY).unwrap();
    let entity_type = state
        .world
        .catalog()
        .entity_type_id_by_key("demo:marker")
        .unwrap();
    let spawned = |state: &State| {
        state
            .entities
            .query_mobile_aabb([2.0, 80.0, 0.0], [3.0, 81.0, 1.0])
            .unwrap()
            .into_iter()
            .filter_map(|id| state.entities.snapshot(id))
            .find(|entity| entity.entity_type == entity_type)
    };
    let (wave, missing) = stage(&mut state, "demo:clock", 1).unwrap();
    assert!(missing.is_empty());
    assert!(spawned(&state).is_none());
    assert!(crate::server::drops::nearby(&state.entities, [3.5, 80.5, 0.5]).is_empty());
    assert_eq!(state.world.cached_block(4, 80, 0), Some(AIR));
    complete_barrier(&mut state, wave.unwrap().barrier()).unwrap();
    assert_eq!(value(&state, "demo:clock"), (1, b"done".to_vec()));
    assert_eq!(
        crate::server::drops::nearby(&state.entities, [3.5, 80.5, 0.5])[0].count,
        2
    );
    assert_eq!(state.world.cached_block(4, 80, 0), Some(GLOWSTONE));
    assert_eq!(
        spawned(&state)
            .unwrap()
            .private_payload
            .downcast_ref::<Vec<u8>>()
            .unwrap(),
        b"x"
    );
    let fingerprint = state.world.catalog().fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x532).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
    });
    let mut restored = fixture.open().unwrap();
    assert_eq!(
        spawned(&restored)
            .unwrap()
            .private_payload
            .downcast_ref::<Vec<u8>>()
            .unwrap(),
        b"x"
    );
    assert_eq!(value(&restored, "demo:clock"), (1, b"done".to_vec()));
    assert_eq!(
        crate::server::drops::nearby(&restored.entities, [3.5, 80.5, 0.5])[0].count,
        2
    );
    assert_eq!(restored.world.get_block(4, 80, 0).unwrap(), GLOWSTONE);
}

#[test]
fn luau_owner_entity_capture_is_immutable_and_restarts() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_entity('demo:marker',1,1,1,nil); h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,reads_entities=true,creates_entities=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) if c.data == 'new' then assert(#c.entities == 0); c.spawn_entity('demo:marker',2,80,0,'x'); return 'read',1 end assert(#c.entities == 1); local e=c.entities[1]; assert(e.key == 'demo:marker' and e.state == 'x' and e.revision_lo == 1 and e.id_lo > 0); assert(e.position[1] == 2.5); assert(not pcall(function() e.state='bad' end)); assert(not pcall(function() e.position[1]=99 end)); return 'done',10000 end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}\nrequires bloxgloom:actions/v1\n"),
    )
    .unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    state.world.get_chunk(KEY).unwrap();
    commit(&mut state, "demo:clock", 1);
    assert_eq!(value(&state, "demo:clock"), (1, b"read".to_vec()));
    commit(&mut state, "demo:clock", 2);
    assert_eq!(value(&state, "demo:clock"), (2, b"done".to_vec()));
    drop(state);
    let restored = fixture.open().unwrap();
    assert_eq!(value(&restored, "demo:clock"), (2, b"done".to_vec()));
}

#[test]
fn luau_owner_entity_update_remove_share_receipts_and_restart() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_entity('demo:marker',1,1,1,nil); h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,reads_entities=true,mutates_entities=true,creates_entities=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) if c.data == 'new' then c.spawn_entity('demo:marker',2,80,0,'x'); return 'change',1 end local e=c.entities[1]; assert(#c.entities == 1 and e.key == 'demo:marker'); if c.data == 'change' then assert(e.state == 'x'); c.update_entity(e.id,e.revision,'y'); return 'remove',1 end assert(e.state == 'y'); c.remove_entity(e.id,e.revision); return 'done',10000 end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}\nrequires bloxgloom:actions/v1\n"),
    )
    .unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    state.world.get_chunk(KEY).unwrap();
    commit(&mut state, "demo:clock", 1);
    let entity_id = state
        .entities
        .query_mobile_aabb([2.0, 80.0, 0.0], [3.0, 81.0, 1.0])
        .unwrap()[0];
    let entity_state = |state: &State| {
        state.entities.snapshot(entity_id).map(|snapshot| {
            snapshot
                .private_payload
                .downcast_ref::<Vec<u8>>()
                .unwrap()
                .clone()
        })
    };
    let (wave, missing) = stage(&mut state, "demo:clock", 2).unwrap();
    assert!(missing.is_empty());
    assert_eq!(entity_state(&state), Some(b"x".to_vec()));
    assert_eq!(value(&state, "demo:clock"), (1, b"change".to_vec()));
    complete_barrier(&mut state, wave.unwrap().barrier()).unwrap();
    assert_eq!(entity_state(&state), Some(b"y".to_vec()));
    assert_eq!(value(&state, "demo:clock"), (2, b"remove".to_vec()));
    drop(state);
    let mut state = Box::new(fixture.open().unwrap());
    state.world.get_chunk(KEY).unwrap();
    assert_eq!(entity_state(&state), Some(b"y".to_vec()));
    let (wave, missing) = stage(&mut state, "demo:clock", 3).unwrap();
    assert!(missing.is_empty());
    assert_eq!(entity_state(&state), Some(b"y".to_vec()));
    complete_barrier(&mut state, wave.unwrap().barrier()).unwrap();
    assert_eq!(entity_state(&state), None);
    assert_eq!(value(&state, "demo:clock"), (3, b"done".to_vec()));
    drop(state);
    let restored = fixture.open().unwrap();
    assert_eq!(entity_state(&restored), None);
    assert_eq!(value(&restored, "demo:clock"), (3, b"done".to_vec()));
}

#[test]
fn luau_owner_caught_invalid_entity_change_rejects_whole_wave() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,reads_entities=true,mutates_entities=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) c.edit(4,80,0,c.block(4,80,0),'bloxgloom:glowstone'); assert(not pcall(function() c.remove_entity(c.tick,c.revision) end)); return 'done',10000 end",
    );
    let mut state = fixture.open().unwrap();
    state.world.get_chunk(KEY).unwrap();
    assert!(stage(&mut state, "demo:clock", 1).is_err());
    assert_eq!(value(&state, "demo:clock"), (0, b"new".to_vec()));
    assert_eq!(state.world.cached_block(4, 80, 0), Some(AIR));
}

#[test]
fn luau_owner_entity_spawn_rejects_obstructed_cell_without_partial_state() {
    let fixture = Fixture::new();
    fixture.system(
        "return function(h) h.register_entity('demo:marker',1,1,1,nil); h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=8,max_jobs_per_tick=1,read_world=true,creates_entities=true,seeds={{x=0,y=5,z=0,data='new'}}} end",
        "return function(c) c.spawn_entity('demo:marker',2,80,0,'x'); return 'done',10000 end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}\nrequires bloxgloom:actions/v1\n"),
    )
    .unwrap();
    let mut state = fixture.open().unwrap();
    state.world.edit(2, 80, 0, STONE).unwrap();
    state.world.get_chunk(KEY).unwrap();
    assert!(stage(&mut state, "demo:clock", 1).is_err());
    assert_eq!(value(&state, "demo:clock"), (0, b"new".to_vec()));
    assert!(
        state
            .entities
            .query_mobile_aabb([2.0, 80.0, 0.0], [3.0, 81.0, 1.0])
            .unwrap()
            .is_empty()
    );
}
const REGISTER: &str = r#"return function(h)
    h.register_system { key='demo:clock', schema=1, revision=1, module='demo:clock',
        max_state_bytes=64, max_jobs_per_tick=2, read_world=true,
        seeds={{x=0,y=5,z=0,data=string.char(0,255)}} }
end"#;
const SOURCE: &str = r#"
local calls = 0
return function(c)
    calls += 1; assert(calls == 1)
    assert(os.clock == nil and os.time == nil and os.date == nil and type(print) == 'function' and require == nil)
    assert(c.owner[1] == 0 and c.owner[2] == 5 and c.owner[3] == 0)
    assert(c.tick_hi == 0 and c.revision_hi == 0)
    assert(string.byte(c.data,2) == 255)
    local n = string.byte(c.data,1)
    local x = if n == 0 then 2 else 3
    local before = c.block(x,80,0)
    local after = if n == 0 then 'bloxgloom:glowstone' else 'bloxgloom:stone'
    if before ~= after then c.edit(x,80,0,before,after) end
    return string.char(n+1,255), 100000
end
"#;

impl Fixture {
    fn system(&self, register: &str, source: &str) {
        self.package(
            "demo",
            "requires bloxgloom:owner_systems/v1\nmodule clock clock.luau",
            register,
        );
        std::fs::write(self.0.join("packages/demo/clock.luau"), source).unwrap();
    }
}

pub(super) fn value(state: &State, system: &str) -> (u64, Vec<u8>) {
    state
        .system_runtime
        .owner_value::<Vec<u8>>(&SystemId::new(system).unwrap(), OwnerKey::Chunk(KEY))
        .unwrap()
}

pub(super) fn stage(
    state: &mut State,
    system: &str,
    tick: u64,
) -> io::Result<(Option<PendingRegisteredWave>, Vec<ChunkKey>)> {
    let registered = state
        .phase_plan
        .system(&SystemId::new(system).unwrap())
        .unwrap()
        .clone();
    let mut missing = Vec::new();
    let wave = state.system_runtime.stage_registered_wave_with_world(
        &registered,
        TickId::new(tick),
        0,
        RegisteredWaveInputs {
            effects: &state.effect_kinds,
            durability: &mut state.durability,
            in_flight: &[],
            world: RegisteredWorldInputs {
                environment: None,
                world: Some(&mut state.world),
                entities: Some(&state.entities),
                lifecycles: Some(&state.lifecycles),
                players: &[],
                seed: state.seed,
                missing: &mut missing,
            },
        },
    )?;
    Ok((wave, missing))
}

pub(super) fn commit(state: &mut State, system: &str, tick: u64) {
    let (wave, missing) = stage(state, system, tick).unwrap();
    assert!(missing.is_empty());
    complete_barrier(state, wave.expect("runnable script owner").barrier()).unwrap();
}

#[test]
fn luau_system_missing_chunk_defers_then_receipt_recovers_bytes_edit_and_deadline() {
    let fixture = Fixture::new();
    fixture.system(REGISTER, SOURCE);
    let mut state = fixture.open().unwrap();
    state
        .world
        .reset_cache_for_test(crate::server::SERVER_CHUNK_CACHE);
    let (wave, missing) = stage(&mut state, "demo:clock", 1).unwrap();
    assert!(wave.is_none());
    assert_eq!(missing, [KEY]);
    assert_eq!(state.world.cached_block(2, 80, 0), None);
    assert_eq!(value(&state, "demo:clock"), (0, vec![0, 255]));
    // Deterministically supply exactly the authoritative input requested by
    // production admission; no sleeps/poll loops to conceal a dropped retry.
    state.world.get_chunk(KEY).unwrap();
    assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
    // Discovery is frozen, including for the retried worker invocation.
    std::fs::write(
        fixture.0.join("packages/demo/clock.luau"),
        "error('mutated')",
    )
    .unwrap();
    let (wave, missing) = stage(&mut state, "demo:clock", 1).unwrap();
    assert!(missing.is_empty());
    assert!(
        state
            .durability
            .reserved
            .contains(&crate::server::durable::chunk_state_key(KEY))
    );
    assert_eq!(value(&state, "demo:clock"), (0, vec![0, 255]));
    assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
    complete_barrier(&mut state, wave.unwrap().barrier()).unwrap();
    assert!(
        !state
            .durability
            .reserved
            .contains(&crate::server::durable::chunk_state_key(KEY))
    );
    assert_eq!(value(&state, "demo:clock"), (1, vec![1, 255]));
    assert_eq!(state.world.cached_block(2, 80, 0), Some(GLOWSTONE));
    assert!(stage(&mut state, "demo:clock", 2).unwrap().0.is_none());
    drop(state);
    fixture.system(REGISTER, SOURCE);
    let mut state = fixture.open().unwrap();
    assert_eq!(value(&state, "demo:clock"), (1, vec![1, 255]));
    assert!(stage(&mut state, "demo:clock", 100000).unwrap().0.is_none());
    assert_eq!(state.world.get_block(2, 80, 0).unwrap(), GLOWSTONE);
    commit(&mut state, "demo:clock", 100001);
    assert_eq!(value(&state, "demo:clock"), (2, vec![2, 255]));
    assert_eq!(state.world.cached_block(2, 80, 0), Some(GLOWSTONE));
    assert_eq!(state.world.cached_block(3, 80, 0), Some(STONE));
}

#[test]
fn luau_system_listener_publishes_only_committed_edit_and_restarts() {
    let fixture = Fixture::new();
    fixture.system(REGISTER, SOURCE);
    for round in 0..2 {
        let mut state = Box::new(fixture.open().unwrap());
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if round == 0 {
            state.world.edit(0, 79, 0, STONE).unwrap();
            state.world.edit(0, 80, 0, AIR).unwrap();
            state.world.edit(0, 81, 0, AIR).unwrap();
            state.world.edit(2, 80, 0, AIR).unwrap();
        } else {
            assert_eq!(value(&state, "demo:clock"), (1, vec![1, 255]));
            assert_eq!(state.world.get_block(2, 80, 0).unwrap(), GLOWSTONE);
        }
        let catalog = state.world.catalog_arc();
        // Reuse the real nonblocking-listener harness, not a mock dispatcher.
        super::gameplay::serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "luau-system".into(),
                    profile: 0x5157,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                assert!(
                    Instant::now() < deadline,
                    "committed script edit never streamed"
                );
                match protocol::read_server_with_catalog(&mut peer, &catalog).unwrap() {
                    ServerMessage::WorldSnapshotStart(snapshot)
                        if snapshot.chunk.key == KEY
                            && snapshot.chunk.block([2, 0, 0]) == Some(GLOWSTONE) =>
                    {
                        break;
                    }
                    ServerMessage::WorldCommitPart(part)
                        if part.key == KEY
                            && part
                                .blocks
                                .iter()
                                .any(|b| b.local == [2, 0, 0] && b.block == GLOWSTONE) =>
                    {
                        break;
                    }
                    _ => {}
                }
            }
        });
    }
    let state = fixture.open().unwrap();
    assert_eq!(value(&state, "demo:clock"), (1, vec![1, 255]));
}

#[test]
fn luau_system_durable_wake_survives_restart_and_runs_before_deadline() {
    let fixture = Fixture::new();
    fixture.system(
        &REGISTER.replace("read_world=true", "read_world=false"),
        "return function(c) return string.char(string.byte(c.data,1)+1,255), 100000 end",
    );
    fixture.package(
        "waker",
        "requires bloxgloom:owner_systems/v1\nmodule clock clock.luau",
        &REGISTER
            .replace("demo:", "waker:")
            .replace("read_world=true", "read_world=false"),
    );
    std::fs::write(
        fixture.0.join("packages/waker/clock.luau"),
        "return function(c) c.wake('demo:clock',0,5,0); return 'sent',100000 end",
    )
    .unwrap();
    let mut state = fixture.open().unwrap();
    commit(&mut state, "demo:clock", 1);
    assert!(stage(&mut state, "demo:clock", 2).unwrap().0.is_none());
    commit(&mut state, "waker:clock", 2);
    drop(state);
    let mut state = fixture.open().unwrap();
    assert_eq!(value(&state, "demo:clock"), (1, vec![1, 255]));
    commit(&mut state, "demo:clock", 3);
    assert_eq!(value(&state, "demo:clock"), (2, vec![2, 255]));
    drop(state);
    let mut state = fixture.open().unwrap();
    assert!(
        stage(&mut state, "demo:clock", 4).unwrap().0.is_none(),
        "served flag must recover cleared"
    );
}
