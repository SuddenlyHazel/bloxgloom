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

#[path = "system/failures.rs"]
mod failures;

const KEY: ChunkKey = ChunkKey { x: 0, y: 5, z: 0 };
const REGISTER: &str = r#"return function(h)
    h.register_system { key='demo:clock', schema=1, revision=1, module='demo:clock',
        max_state_bytes=64, max_jobs_per_tick=2, read_world=true,
        seeds={{x=0,y=5,z=0,data=string.char(0,255)}} }
end"#;
const SOURCE: &str = r#"
local calls = 0
return function(c)
    calls += 1; assert(calls == 1)
    assert(os == nil and print == nil and require == nil)
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

fn value(state: &State, system: &str) -> (u64, Vec<u8>) {
    state
        .system_runtime
        .owner_value::<Vec<u8>>(&SystemId::new(system).unwrap(), OwnerKey::Chunk(KEY))
        .unwrap()
}

fn stage(
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
                world: Some(&mut state.world),
                entities: Some(&state.entities),
                players: &[],
                seed: state.seed,
                missing: &mut missing,
            },
        },
    )?;
    Ok((wave, missing))
}

fn commit(state: &mut State, system: &str, tick: u64) {
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
