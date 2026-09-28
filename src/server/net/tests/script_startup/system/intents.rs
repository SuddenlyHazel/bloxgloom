//! Luau delivery through the public adapter, transaction admission and recovery.
use super::*;
use bloxgloom_host_api::system as api;
#[path = "intent_limits.rs"]
mod limits;

fn registration() -> String {
    REGISTER.replace(
        "read_world=true",
        "read_world=true, accepts_intents=true, intent_bootstrap='new'",
    )
}

const CHAIN: &str = r#"
local calls = 0
return function(c)
    calls += 1; assert(calls == 1)
    assert(table.isfrozen(c.inbox))
    local x = c.owner[1]
    if x == 0 then
        assert(#c.inbox == 0 and c.revision_lo == 0)
    else
        assert(c.data == 'new' and c.revision_lo == 0)
        assert(#c.inbox == 1)
        local m = c.inbox[1]
        assert(table.isfrozen(m) and table.isfrozen(m.id) and table.isfrozen(m.id.source))
        assert(m.id.source[1] == x-1 and m.id.source[2] == 5 and m.id.source[3] == 0)
        assert(m.id.revision_lo == 1 and m.id.revision_hi == 0 and m.id.ordinal == 0)
        assert(m.produced_tick_hi == 0 and m.produced_tick_lo < c.tick_lo)
        assert(m.payload == string.char(0,255))
    end
    local bx = x*16+2
    c.edit(bx,80,0,c.block(bx,80,0),'bloxgloom:glowstone')
    c.wake('demo:clock',x+20,5,0)
    if x < 2 then c.send(x+1,5,0,string.char(0,255)) end
    return 'done',100000
end
"#;

fn owner(x: i32) -> OwnerKey {
    OwnerKey::Chunk(ChunkKey { x, ..KEY })
}
fn id() -> SystemId {
    SystemId::new("demo:clock").unwrap()
}
fn state_value(state: &State, x: i32) -> Option<(u64, Vec<u8>)> {
    state.system_runtime.owner_value::<Vec<u8>>(&id(), owner(x))
}
fn inbox(state: &State, x: i32) -> Vec<api::IntentDelivery> {
    state.system_runtime.intent_inbox_for_test(&id(), owner(x))
}

#[test]
fn luau_intent_retry_receipt_loss_and_restart_preserve_combined_transaction() {
    let fixture = Fixture::new();
    fixture.system(&registration(), CHAIN);
    let mut state = Box::new(fixture.open().unwrap());
    state.world.get_chunk(KEY).unwrap();
    let destination = crate::server::runtime::owner_codec::owner_state_key(&id(), owner(1));
    state.durability.reserved.insert(destination.clone());
    assert_eq!(
        stage(&mut state, "demo:clock", 1).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state_value(&state, 0), Some((0, vec![0, 255])));
    assert!(state_value(&state, 1).is_none());
    assert!(inbox(&state, 1).is_empty());
    assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
    state.durability.reserved.remove(&destination);
    let wave = stage(&mut state, "demo:clock", 2).unwrap().0.unwrap();
    assert!(state_value(&state, 1).is_none());
    assert!(inbox(&state, 1).is_empty());
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(state_value(&state, 0), Some((1, b"done".to_vec())));
    assert_eq!(state_value(&state, 1), Some((0, b"new".to_vec())));
    let message = inbox(&state, 1);
    assert_eq!(
        message,
        vec![api::IntentDelivery {
            id: api::IntentId {
                source: api::Owner::Chunk([0, 5, 0]),
                revision: 1,
                ordinal: 0
            },
            produced_tick: 2,
            payload: vec![0, 255],
        }]
    );
    assert!(stage(&mut state, "demo:clock", 2).unwrap().0.is_none());
    let (wave, missing) = stage(&mut state, "demo:clock", 3).unwrap();
    assert!(wave.is_none());
    assert_eq!(missing, vec![ChunkKey { x: 1, ..KEY }]);
    drop(state);
    let mut state = Box::new(fixture.open().unwrap());
    assert_eq!(inbox(&state, 1), message);
    state.world.get_chunk(ChunkKey { x: 1, ..KEY }).unwrap();
    let chunk_key = crate::server::durable::chunk_state_key(ChunkKey { x: 1, ..KEY });
    state.durability.reserved.insert(chunk_key.clone());
    assert_eq!(
        stage(&mut state, "demo:clock", 4).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(inbox(&state, 1), message);
    assert!(state_value(&state, 2).is_none());
    state.durability.reserved.remove(&chunk_key);
    let wave = stage(&mut state, "demo:clock", 5).unwrap().0.unwrap();
    // Explicit WAL completion, then simulate loss of the coordinator receipt.
    let (tx, rx) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, rx);
    real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    drop(tx);
    assert!(complete_barrier(&mut state, wave.barrier()).is_err());
    assert_eq!(inbox(&state, 1), message);
    assert_eq!(state_value(&state, 1), Some((0, b"new".to_vec())));
    drop(state);
    let journal = crate::server::journal::Journal::open(fixture.0.join("save/server.wal")).unwrap();
    let next_destination = crate::server::runtime::owner_codec::owner_state_key(&id(), owner(2));
    let record = journal
        .records()
        .iter()
        .find(|record| {
            record
                .changes
                .iter()
                .any(|change| change.key == next_destination)
        })
        .unwrap();
    // Forwarding, acknowledgement, conditional terrain, owner bytes and the
    // new absent-owner wake must be participants of the SAME recovered record.
    for key in [
        destination,
        next_destination,
        chunk_key,
        crate::server::runtime::owner_wake::owner_wake_key(&id(), owner(21)),
    ] {
        assert!(record.changes.iter().any(|change| change.key == key));
    }
    assert_eq!(
        record
            .changes
            .iter()
            .filter(|change| { crate::server::runtime::systems::intent::is_key(&change.key) })
            .count(),
        2
    );
    let latest = journal.latest_values();
    for (x, tick) in [(20, 2), (21, 5)] {
        let key = crate::server::runtime::owner_wake::owner_wake_key(&id(), owner(x));
        assert_eq!(
            crate::server::runtime::owner_wake::decode_wake_value(&latest[&key]).unwrap(),
            tick
        );
    }
    drop(journal);
    let mut state = Box::new(fixture.open().unwrap());
    assert!(inbox(&state, 1).is_empty());
    assert_eq!(state_value(&state, 1), Some((1, b"done".to_vec())));
    assert_eq!(state_value(&state, 2), Some((0, b"new".to_vec())));
    assert_eq!(inbox(&state, 2)[0].produced_tick, 5);
    assert_eq!(state.world.get_block(18, 80, 0).unwrap(), GLOWSTONE);
    state.world.get_chunk(ChunkKey { x: 2, ..KEY }).unwrap();
    commit(&mut state, "demo:clock", 6);
    assert!(inbox(&state, 2).is_empty());
    drop(state);
    let mut state = Box::new(fixture.open().unwrap());
    assert!(stage(&mut state, "demo:clock", 7).unwrap().0.is_none());
    for x in 0..=2 {
        assert_eq!(state_value(&state, x), Some((1, b"done".to_vec())));
        assert_eq!(state.world.get_block(x * 16 + 2, 80, 0).unwrap(), GLOWSTONE);
    }
}

#[test]
fn luau_intent_absent_destinations_run_on_real_listener_and_recover_once() {
    let fixture = Fixture::new();
    fixture.system(&registration(), CHAIN);
    for round in 0..2 {
        let mut state = Box::new(fixture.open().unwrap());
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if round == 0 {
            assert!(state_value(&state, 1).is_none());
            assert!(state_value(&state, 2).is_none());
            state.world.edit(0, 79, 0, STONE).unwrap();
        } else {
            for x in 0..=2 {
                assert_eq!(state_value(&state, x), Some((1, b"done".to_vec())));
                assert!(inbox(&state, x).is_empty());
            }
        }
        let catalog = state.world.catalog_arc();
        super::super::gameplay::serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "luau-intent".into(),
                    profile: 0x5158,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            // A streamed final edit is an explicit receipt-completion condition.
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                assert!(Instant::now() < deadline, "intent chain did not commit");
                match protocol::read_server_with_catalog(&mut peer, &catalog).unwrap() {
                    ServerMessage::WorldSnapshotStart(snapshot)
                        if snapshot.chunk.key == ChunkKey { x: 2, ..KEY }
                            && snapshot.chunk.block([2, 0, 0]) == Some(GLOWSTONE) =>
                    {
                        break;
                    }
                    ServerMessage::WorldCommitPart(part)
                        if part.key == ChunkKey { x: 2, ..KEY }
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
    for x in 0..=2 {
        assert_eq!(state_value(&state, x), Some((1, b"done".to_vec())));
        assert!(inbox(&state, x).is_empty());
    }
}

#[test]
fn luau_intent_caught_invalid_and_overbudget_sends_poison_all_output() {
    for body in [
        "c.send(1,5,0,{})",
        "pcall(function() c.send(1.5,5,0,'bad') end)",
        "pcall(function() c.send('other:clock',1,5,0,'bad') end)",
        "pcall(function() c.send(1,5,0,string.rep('x',513)) end)",
        "pcall(function() for i=1,8 do c.send(i,5,0,'overflow') end end)",
        "pcall(function() c.send() end)",
    ] {
        let fixture = Fixture::new();
        fixture.system(
            &registration(),
            &format!(
                r#"return function(c)
            c.send(1,5,0,'valid')
            c.edit(2,80,0,c.block(2,80,0),'bloxgloom:glowstone')
            c.wake('demo:clock',20,5,0)
            {body}
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
        assert_eq!(state_value(&state, 0), Some((0, vec![0, 255])));
        assert!(state_value(&state, 1).is_none());
        assert!(inbox(&state, 1).is_empty());
        assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
        assert_eq!(
            std::fs::read(fixture.0.join("save/server.wal")).unwrap(),
            wal
        );
        drop(state);
        let state = fixture.open().unwrap();
        assert_eq!(state_value(&state, 0), Some((0, vec![0, 255])));
        assert!(state_value(&state, 1).is_none());
    }
}

#[test]
fn luau_intent_declarations_and_session_identity_are_owned_and_bounded() {
    let fixture = Fixture::new();
    for register in [
        registration().replace("accepts_intents=true", "accepts_intents=1"),
        registration().replace("accepts_intents=true", "accepts_intents=false"),
        registration().replace("read_world=true", "read_world=false"),
        registration().replace("intent_bootstrap='new'", "intent_bootstrap={}"),
        registration().replace(
            "intent_bootstrap='new'",
            "intent_bootstrap=string.rep('x',65)",
        ),
    ] {
        fixture.system(&register, CHAIN);
        assert!(fixture.open().is_err(), "accepted {register}");
        assert!(!fixture.0.join("save").exists());
    }
    fixture.system(&registration(), CHAIN);
    let state = fixture.open().unwrap();
    let fingerprint = state.world.catalog_arc().fingerprint();
    let client = state
        .client_bundle
        .as_ref()
        .unwrap()
        .session_catalog()
        .unwrap();
    assert_eq!(client.fingerprint(), fingerprint);
    assert_eq!(
        client.owner_systems().count(),
        0,
        "client gained owner execution"
    );
    drop(state);
    let files = ["content.map", "server.wal"]
        .map(|file| std::fs::read(fixture.0.join("save").join(file)).unwrap());
    for register in [
        registration().replace("'new'", "'changed'"),
        REGISTER.to_owned(),
    ] {
        fixture.system(&register, CHAIN);
        assert!(fixture.open().is_err(), "changed identity reopened save");
        for (file, bytes) in ["content.map", "server.wal"].iter().zip(&files) {
            assert_eq!(
                &std::fs::read(fixture.0.join("save").join(file)).unwrap(),
                bytes
            );
        }
        let other = Fixture::new();
        other.system(&register, CHAIN);
        let other = other.open().unwrap();
        assert_ne!(other.world.catalog_arc().fingerprint(), fingerprint);
    }
}
