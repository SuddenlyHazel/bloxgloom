use super::*;
use crate::server::durable::{CommitAction, CommitBarrier, complete_barrier};
use crate::server::runtime::systems::{
    PendingRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs,
};
use crate::server::simulation::TickId;
use crate::server::startup::ServerStartup;
use crate::server::{State, server_state_with_startup};
use crate::world::{ChunkKey, GLOWSTONE, SAND};
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, system as api};
use std::sync::Arc;
use std::time::Duration;

mod bootstrap;
mod listener;

const SYSTEM: &str = "test:ignition_intents";
fn system_id() -> SystemId {
    SystemId::new(SYSTEM).unwrap()
}
fn chunk(x: i32) -> ChunkKey {
    ChunkKey { x, y: 6, z: 0 }
}
fn owner(x: i32) -> OwnerKey {
    OwnerKey::Chunk(chunk(x))
}
fn cell(x: i32) -> [i32; 3] {
    [x * 16 + 8, 100, 8]
}

#[derive(Clone)]
struct Ignitions {
    fanout: i32,
    bootstrap: bool,
    observed: Option<std::sync::mpsc::Sender<()>>,
}
impl api::Behavior for Ignitions {
    fn intent_bootstrap(&self) -> Option<&[u8]> {
        self.bootstrap.then_some(&[0])
    }
    fn accepts_intents(&self) -> bool {
        true
    }
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() == 1 {
            Ok(())
        } else {
            Err(RegistrationError("expected one counter byte".into()))
        }
    }
    fn plan(&self, _: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        Err(RegistrationError(
            "intent-aware entry point required".into(),
        ))
    }
    fn plan_with_intents(
        &self,
        c: &api::Context<'_>,
        inbox: &[IntentDelivery],
        outbox: &mut api::IntentOutbox,
    ) -> Result<api::Plan, RegistrationError> {
        let api::Owner::Chunk([x, 6, 0]) = c.owner else {
            return Err(RegistrationError("unexpected owner".into()));
        };
        let mut data = c.data[0];
        let mut edits = vec![];
        if x == 8 && data == 0 {
            for target in 9..9 + self.fanout {
                outbox.send(api::Owner::Chunk([target, 6, 0]), &[1])?;
            }
            data = 1;
        }
        if !inbox.is_empty() {
            for message in inbox {
                if message.payload != [1] {
                    return Err(RegistrationError("invalid ignition payload".into()));
                }
                data += 1;
            }
            let block = c
                .block(cell(x))
                .map_err(|e| RegistrationError(e.to_string()))?;
            if block.state != "bloxgloom:glowstone" {
                edits.push(api::BlockEdit {
                    cell: cell(x),
                    before: block.state,
                    after: "bloxgloom:glowstone".into(),
                });
            }
            if x == 9 && self.fanout == 1 {
                outbox.send(api::Owner::Chunk([10, 6, 0]), &[1])?;
            }
            if x == 10
                && let Some(observed) = &self.observed
            {
                let _ = observed.send(());
            }
        }
        Ok(api::Plan {
            data: vec![data],
            next_tick: c.tick + 10_000,
            wakes: vec![],
            edits,
        })
    }
}
impl Extension for Ignitions {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.owner_system(self.definition())
    }
}
impl Ignitions {
    fn definition(&self) -> api::System {
        api::System {
            key: SYSTEM.into(),
            schema: 1,
            partition: api::Partition::Chunk,
            max_state_bytes: 1,
            max_jobs_per_tick: 1,
            read_radius_chunks: Some(0),
            after: vec![],
            seeds: (8..=if self.bootstrap {
                8
            } else {
                (8 + self.fanout).max(10)
            })
                .map(|x| api::Seed {
                    owner: api::Owner::Chunk([x, 6, 0]),
                    data: vec![0],
                })
                .collect(),
            behavior: Arc::new(self.clone()),
        }
    }
}
fn startup(fanout: i32) -> ServerStartup {
    startup_with_bootstrap(fanout, false)
}
fn startup_with_bootstrap(fanout: i32, bootstrap: bool) -> ServerStartup {
    ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&Ignitions {
            fanout,
            bootstrap,
            observed: None,
        })
        .unwrap()
}
fn open(path: &std::path::Path, fanout: i32) -> Box<State> {
    Box::new(server_state_with_startup(7, path.to_path_buf(), 2, startup(fanout)).unwrap())
}
fn open_with_bootstrap(path: &std::path::Path, fanout: i32, bootstrap: bool) -> Box<State> {
    Box::new(
        server_state_with_startup(
            7,
            path.to_path_buf(),
            2,
            startup_with_bootstrap(fanout, bootstrap),
        )
        .unwrap(),
    )
}
fn save() -> std::path::PathBuf {
    static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "bloxgloom-intents-{}-{}-{}",
        std::process::id(),
        NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn load(state: &mut State, x: i32) {
    let [x, y, z] = cell(x);
    state.world.get_block(x, y, z).unwrap();
}
fn value(state: &State, x: i32) -> u8 {
    state
        .system_runtime
        .owner_value::<Vec<u8>>(&system_id(), owner(x))
        .unwrap()
        .1[0]
}
fn pending(state: &State, x: i32) -> Mailbox {
    state.system_runtime.durable_wakes.intents.capture(
        &system_id(),
        owner(x),
        u64::MAX,
        MAX_WAVE_INTENTS,
    )
}
fn stage(
    state: &mut State,
    tick: u64,
) -> io::Result<(Option<PendingRegisteredWave>, Vec<ChunkKey>)> {
    stage_with_in_flight(state, tick, &[])
}
fn stage_with_in_flight(
    state: &mut State,
    tick: u64,
    in_flight: &[Vec<StateKey>],
) -> io::Result<(Option<PendingRegisteredWave>, Vec<ChunkKey>)> {
    let registered = state.phase_plan.system(&system_id()).unwrap().clone();
    let mut missing = vec![];
    let wave = state.system_runtime.stage_registered_wave_with_world(
        &registered,
        TickId::new(tick),
        0,
        RegisteredWaveInputs {
            effects: &state.effect_kinds,
            durability: &mut state.durability,
            in_flight,
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

#[test]
fn durable_intent_cross_chunk_chain_gates_delivery_ack_cancel_and_restart() {
    ignition_chain(false);
}

#[test]
fn durable_intent_bootstrap_chain_gates_creation_retry_forwarding_and_restart() {
    ignition_chain(true);
}

fn ignition_chain(bootstrap: bool) {
    let path = save();
    let mut state = open_with_bootstrap(&path, 1, bootstrap);
    load(&mut state, 8);
    state.durability.rotation_requested = true;
    assert_eq!(
        stage(&mut state, 1).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert!(state.system_runtime.durable_wakes.intents.staged.is_empty());
    assert_eq!(state.system_runtime.durable_wakes.intents.staged_added, 0);
    assert_eq!(value(&state, 8), 0);
    if bootstrap {
        assert!(
            state
                .system_runtime
                .owner_snapshot(&system_id(), owner(9))
                .is_none()
        );
    }
    state.durability.rotation_requested = false;
    let (wave, missing) = stage(&mut state, 4).unwrap();
    assert!(missing.is_empty());
    let wave = wave.unwrap();
    assert!(pending(&state, 9).is_empty());
    assert_eq!(value(&state, 8), 0);
    if bootstrap {
        assert!(
            state
                .system_runtime
                .owner_snapshot(&system_id(), owner(9))
                .is_none()
        );
    }
    // Same still-uncommitted producer revision cannot submit a duplicate.
    assert_eq!(
        stage(&mut state, 5).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    complete_barrier(&mut state, wave.barrier()).unwrap();
    let identity = pending(&state, 9)[0].id;
    assert_eq!(identity.revision, 1);
    assert_eq!(pending(&state, 9)[0].produced_tick, 4);
    assert_eq!(value(&state, 8), 1);
    assert_eq!(
        state
            .system_runtime
            .owner_snapshot(&system_id(), owner(9))
            .unwrap()
            .0,
        0
    );
    if bootstrap {
        assert!(
            stage(&mut state, 4).unwrap().0.is_none(),
            "bootstrap cannot plan in producing tick"
        );
    }
    let (wave, missing) = stage(&mut state, 6).unwrap();
    assert!(wave.is_none());
    assert_eq!(missing, [chunk(9)]);
    assert_eq!(pending(&state, 9)[0].id, identity);
    drop(state);

    let mut state = open_with_bootstrap(&path, 1, bootstrap);
    assert_eq!(pending(&state, 9)[0].id, identity);
    assert_eq!(value(&state, 9), 0);
    assert!(state.world.cached_chunk(chunk(9)).is_none());
    load(&mut state, 9);
    // A competing terrain writer cancels acknowledgement and forwarding as a
    // unit, retaining the message and destination state for a fresh plan.
    let [x, y, z] = cell(9);
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: state.world.prepare_edits(&[(x + 1, y, z, SAND)]).unwrap(),
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        entity_wakes: vec![],
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(8), &action, None)
            .unwrap()
    );
    assert_eq!(
        stage(&mut state, 8).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(pending(&state, 9)[0].id, identity);
    assert!(pending(&state, 10).is_empty());
    if bootstrap {
        assert!(
            state
                .system_runtime
                .owner_snapshot(&system_id(), owner(10))
                .is_none()
        );
    }
    assert_eq!(value(&state, 9), 0);
    assert!(state.system_runtime.durable_wakes.intents.staged.is_empty());
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    let wave = stage(&mut state, 9).unwrap().0.unwrap();
    assert_eq!(pending(&state, 9)[0].id, identity);
    assert!(pending(&state, 10).is_empty());
    // Confirm the real WAL write but lose the coordinator apply. On restart,
    // destination edit/state, ack and forwarded payload must recover together.
    let (tx, rx) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, rx);
    real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    drop(tx);
    assert!(complete_barrier(&mut state, wave.barrier()).is_err());
    assert_eq!(value(&state, 9), 0);
    drop(state);

    let mut state = open_with_bootstrap(&path, 1, bootstrap);
    assert_eq!(value(&state, 9), 1);
    assert!(pending(&state, 9).is_empty());
    assert_eq!(pending(&state, 10).len(), 1);
    assert_eq!(state.world.get_block(x, y, z).unwrap(), GLOWSTONE);
    load(&mut state, 10);
    let wave = stage(&mut state, 11).unwrap().0.unwrap();
    assert_eq!(value(&state, 10), 0);
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(value(&state, 10), 1);
    assert!(pending(&state, 10).is_empty());
    drop(state);
    let mut state = open_with_bootstrap(&path, 1, bootstrap);
    assert_eq!(
        (value(&state, 8), value(&state, 9), value(&state, 10)),
        (1, 1, 1)
    );
    assert!(
        stage(&mut state, 14).unwrap().0.is_none(),
        "acknowledged payload must not be redelivered"
    );
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn durable_intent_inspection_rotates_past_distinct_unavailable_destinations() {
    inspection_rotates(false);
}

#[test]
fn durable_intent_bootstrap_inspection_rotates_past_unavailable_destinations() {
    inspection_rotates(true);
}

fn inspection_rotates(bootstrap: bool) {
    let path = save();
    let mut state = open_with_bootstrap(&path, 8, bootstrap);
    load(&mut state, 8);
    load(&mut state, 16);
    let wave = stage(&mut state, 1).unwrap().0.unwrap();
    complete_barrier(&mut state, wave.barrier()).unwrap();
    // Eight candidates, two bounded mailbox turns out of every three ticks.
    // No sleep/polling: all seven blocked chunks stay deliberately unavailable.
    for tick in 2..=12 {
        if let Some(wave) = stage(&mut state, tick).unwrap().0 {
            complete_barrier(&mut state, wave.barrier()).unwrap();
        }
    }
    assert_eq!(value(&state, 16), 1);
    for x in 9..16 {
        assert_eq!(pending(&state, x).len(), 1);
        assert_eq!(value(&state, x), 0);
    }
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

fn message(source: i32, revision: u64) -> IntentDelivery {
    IntentDelivery {
        id: api::IntentId {
            source: api::Owner::Chunk([source, 6, 0]),
            revision,
            ordinal: 0,
        },
        produced_tick: revision,
        payload: vec![1],
    }
}

#[test]
fn durable_intent_capacity_duplicate_collision_and_cancel_are_atomic() {
    let system = system_id();
    let mut store = IntentStore::default();
    for x in 0..(MAX_PENDING_INTENTS / MAX_MAILBOX_INTENTS) as i32 {
        let outputs: Vec<_> = (1..=MAX_MAILBOX_INTENTS as u64)
            .map(|revision| (owner(x), message(x, revision)))
            .collect();
        let prepared = store.prepare(&system, &[], &outputs).unwrap();
        store.commit(prepared).unwrap();
    }
    let duplicate = store
        .prepare(&system, &[], &[(owner(0), message(0, 1))])
        .unwrap();
    assert!(duplicate.changes().is_empty());
    store.cancel(duplicate);
    let mut collision = message(0, 1);
    collision.payload = vec![2];
    assert_eq!(
        store
            .prepare(&system, &[], &[(owner(0), collision)])
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        store
            .prepare(&system, &[], &[(owner(0), message(0, 65))])
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(
        store
            .prepare(&system, &[], &[(owner(40), message(0, 65))])
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    // A full global queue still permits same-record acknowledgement/forwarding.
    let received = vec![(owner(0), store.capture(&system, owner(0), 100, 1))];
    let outputs = vec![(owner(40), message(0, 65))];
    let prepared = store.prepare(&system, &received, &outputs).unwrap();
    assert_eq!(store.count, MAX_PENDING_INTENTS);
    assert_eq!(
        store
            .prepare(&system, &received, &outputs)
            .unwrap_err()
            .kind(),
        ErrorKind::WouldBlock
    );
    let changes = prepared.changes().to_vec();
    store.cancel(prepared);
    assert!(store.staged.is_empty());
    assert_eq!(store.staged_added, 0);
    let prepared = store.prepare(&system, &received, &outputs).unwrap();
    assert_eq!(
        prepared
            .changes()
            .iter()
            .map(|c| &c.after)
            .collect::<Vec<_>>(),
        changes.iter().map(|c| &c.after).collect::<Vec<_>>()
    );
    store.commit(prepared).unwrap();
    assert_eq!(store.count, MAX_PENDING_INTENTS);
    assert_eq!(store.capture(&system, owner(40), 100, 8).len(), 1);
    assert_eq!(store.capture(&system, owner(0), 100, 1)[0].id.revision, 2);
}

#[test]
fn durable_intent_codec_and_public_outbox_fail_closed_without_changing_wakes() {
    let mut outbox = api::IntentOutbox::default();
    assert!(
        outbox
            .send(
                api::Owner::Chunk([0, 0, 0]),
                &vec![0; api::MAX_INTENT_PAYLOAD_BYTES + 1]
            )
            .is_err()
    );
    assert!(outbox.finish().is_err());
    let mailbox = vec![message(8, 1)];
    let bytes = codec::encode(&mailbox).unwrap();
    assert_eq!(codec::decode(&bytes).unwrap(), mailbox);
    for length in 1..bytes.len() {
        assert!(codec::decode(&bytes[..length]).is_err());
    }
    let key = codec::key(&system_id(), owner(9));
    let mut latest = BTreeMap::new();
    latest.insert(key, bytes);
    latest.insert(
        crate::server::runtime::owner_wake::owner_wake_key(&system_id(), owner(9)),
        crate::server::runtime::owner_wake::encode_wake_value(7),
    );
    let recovered = crate::server::runtime::owner_wake::PendingWakeStore::recover(&latest).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        recovered.intents.capture(&system_id(), owner(9), 2, 8),
        mailbox
    );
    assert!(
        recovered
            .intents
            .capture(&system_id(), owner(9), 1, 8)
            .is_empty()
    );

    let mut declaration = bloxgloom_lifecycle_fixture::system::definition();
    declaration.read_radius_chunks = Some(0);
    let old_fingerprint = declaration.fingerprint_bytes();
    declaration.behavior = Arc::new(Ignitions {
        fanout: 1,
        bootstrap: false,
        observed: None,
    });
    let enabled_fingerprint = declaration.fingerprint_bytes();
    assert_ne!(enabled_fingerprint, old_fingerprint);
    assert!(enabled_fingerprint.starts_with(&old_fingerprint));
    declaration.read_radius_chunks = None;
    assert!(
        declaration.validate().is_err(),
        "intent support requires the live chunk deferral path"
    );
}

#[test]
fn durable_intent_recovery_rejects_impossible_or_duplicated_producer_identity() {
    for duplicate in [false, true] {
        let path = save();
        let mut state = open(&path, 1);
        load(&mut state, 8);
        let wave = stage(&mut state, 1).unwrap().0.unwrap();
        complete_barrier(&mut state, wave.barrier()).unwrap();
        let message = if duplicate {
            pending(&state, 9).remove(0)
        } else {
            message(8, 100)
        };
        drop(state);
        let journal = crate::server::journal::Journal::open(path.join("server.wal")).unwrap();
        let id = journal.next_id().unwrap();
        let writer = journal.into_writer(8, Duration::ZERO).unwrap();
        // Structurally valid bytes cannot assert a producer revision that never
        // committed. This check runs in recovery before any save replay writes.
        let change = Change::new(
            codec::key(&system_id(), owner(10)),
            vec![],
            codec::encode(&[message]).unwrap(),
        );
        writer
            .try_submit(crate::server::journal::Transaction::new(
                id,
                100,
                vec![change],
            ))
            .unwrap()
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        writer.shutdown().unwrap();
        let error = match server_state_with_startup(7, path.clone(), 2, startup(1)) {
            Ok(_) => panic!("uncommitted producer was accepted"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(error.to_string().contains(if duplicate {
            "multiple mailboxes"
        } else {
            "producer revision"
        }));
        std::fs::remove_dir_all(path).unwrap();
    }
}
