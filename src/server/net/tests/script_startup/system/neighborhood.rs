//! Actual package files -> frozen public adapter -> capture -> worker -> WAL/TCP.
use super::*;

#[path = "neighborhood/failures.rs"]
mod failures;

const SYSTEM: &str = "relay:grow";
const TARGET: ChunkKey = ChunkKey { x: 2, ..KEY };
const MAIN: &str = include_str!("../../../../../../fixtures/neighborhood/packages/relay/main.luau");
const GROW: &str = include_str!("../../../../../../fixtures/neighborhood/packages/relay/grow.luau");
const MANIFEST: &str =
    include_str!("../../../../../../fixtures/neighborhood/packages/relay/package.txt");

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    let package = fixture.0.join("packages/relay");
    std::fs::create_dir(&package).unwrap();
    for (name, contents) in [
        ("package.txt", MANIFEST),
        ("main.luau", MAIN),
        ("grow.luau", GROW),
    ] {
        std::fs::write(package.join(name), contents).unwrap();
    }
    fixture
}

fn owner(x: i32) -> OwnerKey {
    OwnerKey::Chunk(ChunkKey { x, ..KEY })
}
fn id() -> SystemId {
    SystemId::new(SYSTEM).unwrap()
}
fn state_value(state: &State, x: i32) -> Option<(u64, Vec<u8>)> {
    state.system_runtime.owner_value::<Vec<u8>>(&id(), owner(x))
}
fn inbox(state: &State, x: i32) -> Vec<bloxgloom_host_api::system::IntentDelivery> {
    state.system_runtime.intent_inbox_for_test(&id(), owner(x))
}

fn load_neighborhoods(state: &mut State, except: Option<ChunkKey>) {
    // Exactly the union of two radius-one captures, not a procedural read view.
    for x in -1..=2 {
        for y in 4..=6 {
            for z in -1..=1 {
                let key = ChunkKey { x, y, z };
                if Some(key) != except {
                    state.world.get_chunk(key).unwrap();
                }
            }
        }
    }
}

#[test]
fn luau_neighborhood_unavailable_retry_read_fence_and_lost_receipt_recover_atomically() {
    let fixture = fixture();
    let mut state = Box::new(fixture.open().unwrap());
    state
        .world
        .reset_cache_for_test(crate::server::SERVER_CHUNK_CACHE);
    load_neighborhoods(&mut state, Some(TARGET));
    commit(&mut state, SYSTEM, 1);
    assert_eq!(state_value(&state, 0), Some((1, b"sent".to_vec())));
    assert_eq!(state_value(&state, 1), Some((0, b"new".to_vec())));
    let message = inbox(&state, 1);
    assert_eq!(message.len(), 1);
    assert_eq!(message[0].produced_tick, 1);
    assert_eq!(message[0].payload, b"bloxgloom:air");
    assert!(stage(&mut state, SYSTEM, 1).unwrap().0.is_none());
    let wal = std::fs::read(fixture.0.join("save/server.wal")).unwrap();
    let (wave, missing) = stage(&mut state, SYSTEM, 2).unwrap();
    assert!(wave.is_none());
    assert_eq!(missing, [TARGET]);
    assert_eq!(state.world.cached_block(34, 80, 2), None);
    assert_eq!(inbox(&state, 1), message);
    assert_eq!(
        std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
        wal
    );
    // Recovery retains the undelivered intent and its revision-zero bootstrap.
    drop(state);
    let mut state = Box::new(fixture.open().unwrap());
    load_neighborhoods(&mut state, None);
    assert_eq!(state.world.cached_block(34, 80, 2), Some(AIR));
    assert_eq!(inbox(&state, 1), message);
    let read_only = crate::server::durable::chunk_state_key(ChunkKey { x: 0, y: 4, z: -1 });
    state.durability.reserved.insert(read_only.clone());
    assert_eq!(
        stage(&mut state, SYSTEM, 3).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state_value(&state, 1), Some((0, b"new".to_vec())));
    assert_eq!(inbox(&state, 1), message);
    state.durability.reserved.remove(&read_only);
    let wave = stage(&mut state, SYSTEM, 4).unwrap().0.unwrap();
    assert!(state.durability.reserved.contains(&read_only));
    let chunk_key = crate::server::durable::chunk_state_key(TARGET);
    assert!(state.durability.reserved.contains(&chunk_key));
    assert_eq!(state.world.cached_block(34, 80, 2), Some(AIR));
    assert_eq!(inbox(&state, 1), message);
    // Wait for actual WAL completion, then lose just the coordinator receipt.
    let (tx, rx) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, rx);
    real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    drop(tx);
    assert!(complete_barrier(&mut state, wave.barrier()).is_err());
    assert_eq!(state.world.cached_block(34, 80, 2), Some(AIR));
    assert_eq!(inbox(&state, 1), message);
    drop(state);
    let journal = crate::server::journal::Journal::open(fixture.0.join("save/server.wal")).unwrap();
    let record = journal
        .records()
        .iter()
        .find(|record| record.changes.iter().any(|change| change.key == chunk_key))
        .unwrap();
    assert!(record.changes.iter().any(|change| change.key
        == crate::server::runtime::owner_codec::owner_state_key(&id(), owner(1))));
    assert_eq!(
        record
            .changes
            .iter()
            .filter(|change| crate::server::runtime::systems::intent::is_key(&change.key))
            .count(),
        1
    );
    drop(journal);
    let mut state = Box::new(fixture.open().unwrap());
    assert_eq!(state.world.get_block(34, 80, 2).unwrap(), GLOWSTONE);
    assert_eq!(state_value(&state, 1), Some((1, b"done".to_vec())));
    assert!(inbox(&state, 1).is_empty());
    assert!(stage(&mut state, SYSTEM, 5).unwrap().0.is_none());
}

#[test]
fn luau_neighborhood_package_runs_through_nonblocking_listener_and_restart() {
    let fixture = fixture();
    for round in 0..2 {
        let mut state = Box::new(fixture.open().unwrap());
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if round == 0 {
            state.world.edit(0, 79, 0, STONE).unwrap();
            assert!(state_value(&state, 1).is_none());
        } else {
            assert_eq!(state_value(&state, 1), Some((1, b"done".to_vec())));
            assert!(inbox(&state, 1).is_empty());
        }
        let catalog = state.world.catalog_arc();
        let client = state
            .client_bundle
            .as_ref()
            .unwrap()
            .session_catalog()
            .unwrap();
        assert_eq!(client.fingerprint(), catalog.fingerprint());
        assert_eq!(client.owner_systems().count(), 0);
        super::super::gameplay::serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "neighborhood".into(),
                    profile: 0x5159,
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
                    "neighborhood relay never committed"
                );
                match protocol::read_server_with_catalog(&mut peer, &catalog).unwrap() {
                    ServerMessage::WorldSnapshotStart(snapshot)
                        if snapshot.chunk.key == TARGET
                            && snapshot.chunk.block([2, 0, 2]) == Some(GLOWSTONE) =>
                    {
                        break;
                    }
                    ServerMessage::WorldCommitPart(part)
                        if part.key == TARGET
                            && part
                                .blocks
                                .iter()
                                .any(|b| b.local == [2, 0, 2] && b.block == GLOWSTONE) =>
                    {
                        break;
                    }
                    _ => {}
                }
            }
        });
    }
    let mut state = fixture.open().unwrap();
    assert_eq!(state_value(&state, 0), Some((1, b"sent".to_vec())));
    assert_eq!(state_value(&state, 1), Some((1, b"done".to_vec())));
    assert!(inbox(&state, 1).is_empty());
    assert_eq!(state.world.get_block(34, 80, 2).unwrap(), GLOWSTONE);
}
