//! Mixed-producer regressions through the live admission and receipt gate.
use super::*;
use crate::server::durable::{CommitBarrier, complete_barrier};
use crate::server::parallel::{
    OwnerData, OwnerJob, OwnerKey, OwnerPatch, OwnerSchedule, PatchUsage,
};
use crate::server::registry::{OwnerPartition, ResourceId, SystemDescriptor, SystemId};
use crate::server::runtime::owner_codec::{OwnerCodecError, OwnerValueCodec, owner_state_key};
use crate::server::runtime::systems::PendingRegisteredWave;
use crate::server::simulation::Phase;
use crate::server::startup::StartupOwnerCodec;
use std::time::Duration;

struct OwnerCodec;
impl OwnerValueCodec for OwnerCodec {
    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        Ok(value
            .get::<u64>()
            .ok_or(OwnerCodecError::InvalidData)?
            .to_le_bytes()
            .to_vec())
    }
    fn decode(&self, bytes: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        Ok(OwnerData::new(u64::from_le_bytes(
            bytes.try_into().map_err(|_| OwnerCodecError::InvalidData)?,
        )))
    }
}

fn owner() -> OwnerKey {
    OwnerKey::Chunk(crate::world::ChunkKey { x: 0, y: 0, z: 0 })
}
fn system() -> SystemId {
    SystemId::new("test:mixed_commits").unwrap()
}

fn mixed_startup() -> crate::server::startup::ServerStartup {
    let mut startup = startup(false);
    startup.register_system(
        SystemDescriptor::new(system(), Phase::Simulation, OwnerPartition::Chunk, 1, 0)
            .write(ResourceId::new("test:mixed_state").unwrap()),
        |job: &OwnerJob| {
            let value = job
                .snapshot(job.owner())
                .unwrap()
                .value::<OwnerData>()
                .unwrap()
                .get::<u64>()
                .unwrap();
            Ok(OwnerPatch::new(
                job,
                OwnerData::new(value + 1),
                PatchUsage {
                    writes: 1,
                    effects: 0,
                    estimated_bytes: 8,
                },
            )
            .with_schedule(OwnerSchedule::AtTick(20)))
        },
    );
    startup.register_owner_codec(
        system(),
        StartupOwnerCodec {
            codec: Arc::new(OwnerCodec),
            codec_version: 1,
            max_bytes: 8,
        },
    );
    startup.seed_owner(system(), owner(), 10u64);
    startup
}

fn stage_owner(state: &mut State) -> io::Result<Option<PendingRegisteredWave>> {
    stage_owner_attempt(state, 0)
}

fn stage_owner_attempt(
    state: &mut State,
    attempt: u16,
) -> io::Result<Option<PendingRegisteredWave>> {
    let registered = state.phase_plan.system(&system()).unwrap().clone();
    state.system_runtime.stage_registered_wave(
        &registered,
        TickId::new(6),
        attempt,
        &state.effect_kinds,
        &mut state.durability,
        &[],
    )
}

#[test]
fn mixed_receipts_wait_in_admission_order_and_barrier_stops_at_its_frontier() {
    let path = temp_save_dir("mixed-ordered-barrier");
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 1, mixed_startup()).unwrap();
    let ids = seed(&mut state, &[[1.5, 100.5, 1.5], [3.5, 100.5, 1.5]]);
    let before = state.entities.revision();
    let first = update(&state, ids[0]);
    stage(&mut state, &first).unwrap();
    let wave = stage_owner(&mut state).unwrap().unwrap();
    let last = update(&state, ids[1]);
    stage(&mut state, &last).unwrap();
    let (release_first, receipt_first) = hold(&mut state, 0);
    let (release_owner, receipt_owner) = hold(&mut state, 1);
    let (release_last, receipt_last) = hold(&mut state, 2);
    release_last
        .send(receipt_last.recv_timeout(Duration::from_secs(5)).unwrap())
        .unwrap();
    release_owner
        .send(receipt_owner.recv_timeout(Duration::from_secs(5)).unwrap())
        .unwrap();
    crate::server::durable::poll_journal_receipts(&mut state).unwrap();
    assert_eq!(state.entities.revision(), before);
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system(), owner()),
        Some((0, 10))
    );
    assert!(state.durability.publish_queue.is_empty());
    release_first
        .send(receipt_first.recv_timeout(Duration::from_secs(5)).unwrap())
        .unwrap();
    let applied = complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(applied.commits, 2);
    assert_eq!(applied.owner_writes, 1);
    assert_eq!(state.entities.revision(), before + 1);
    assert_eq!(state.durability.publish_queue.len(), 1);
    assert_eq!(
        state.durability.pending.len(),
        1,
        "ready suffix stays behind the explicit frontier"
    );
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system(), owner()),
        Some((1, 11))
    );
    assert!(
        stage_owner(&mut state).unwrap().is_none(),
        "receipted deadline suppresses early rerun"
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    assert_eq!(state.entities.revision(), before + 2);
    assert!(state.durability.reserved.is_empty());
    drop(state);
    let mut recovered =
        crate::server::server_state_with_startup(7, path.clone(), 1, mixed_startup()).unwrap();
    assert_eq!(recovered.entities.revision(), before + 2);
    assert_eq!(
        recovered
            .system_runtime
            .owner_value::<u64>(&system(), owner()),
        Some((1, 11))
    );
    assert!(stage_owner(&mut recovered).unwrap().is_none());
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn shared_entity_reads_fence_owner_writes_until_last_apply_and_rejection_leaks_nothing() {
    let path = temp_save_dir("mixed-read-reservation");
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 1, mixed_startup()).unwrap();
    let ids = seed(&mut state, &[[1.5, 100.5, 1.5], [3.5, 100.5, 1.5]]);
    let key = owner_state_key(&system(), owner());
    for id in ids {
        let mut action = update(&state, id);
        action.entities.as_mut().unwrap().add_read_key(key.clone());
        stage(&mut state, &action).unwrap();
    }
    let frontier = state.durability.pending[0].id;
    let next_id = state.durability.next_id;
    assert_eq!(
        stage_owner(&mut state).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state.durability.next_id, next_id);
    complete_barrier(&mut state, CommitBarrier::Through(frontier)).unwrap();
    assert!(state.durability.reserved.contains(&key));
    assert_eq!(
        stage_owner_attempt(&mut state, 1).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    assert!(!state.durability.reserved.contains(&key));
    // Rotation rejection uses the same rollback path and retains due work.
    state.durability.rotation_requested = true;
    assert_eq!(
        stage_owner_attempt(&mut state, 2).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state.durability.next_id, next_id);
    state.durability.rotation_requested = false;
    let wave = stage_owner_attempt(&mut state, 3).unwrap().unwrap();
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system(), owner()),
        Some((1, 11))
    );
    assert!(state.durability.reserved.is_empty());
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn confirmed_entity_prefix_survives_owner_receipt_failure_and_suffix_is_quarantined() {
    for disconnected in [false, true] {
        let path = temp_save_dir("mixed-failed-receipt");
        let mut state =
            crate::server::server_state_with_startup(7, path.clone(), 1, mixed_startup()).unwrap();
        let ids = seed(&mut state, &[[1.5, 100.5, 1.5], [3.5, 100.5, 1.5]]);
        let before = state.entities.revision();
        let first = update(&state, ids[0]);
        stage(&mut state, &first).unwrap();
        let wave = stage_owner(&mut state).unwrap().unwrap();
        let last = update(&state, ids[1]);
        stage(&mut state, &last).unwrap();
        let (release, real) = hold(&mut state, 1);
        real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        if !disconnected {
            release
                .send(Err(io::Error::other("injected receipt failure")))
                .unwrap();
        }
        drop(release);
        assert!(complete_barrier(&mut state, wave.barrier()).is_err());
        assert!(state.durability.failed);
        assert_eq!(state.entities.revision(), before + 1);
        assert_eq!(state.durability.publish_queue.len(), 1);
        assert_eq!(
            state.system_runtime.owner_value::<u64>(&system(), owner()),
            Some((0, 10))
        );
        assert_eq!(state.durability.pending.len(), 2);
        assert!(crate::server::durable::poll_journal_receipts(&mut state).is_err());
        assert_eq!(
            state.entities.revision(),
            before + 1,
            "failed gate cannot publish a ready suffix"
        );
        drop(state);
        // The injected receipt failure hides a real accepted WAL record.
        // Replay, not speculative rollback or the missing receipt, is truth.
        let recovered =
            crate::server::server_state_with_startup(7, path.clone(), 1, mixed_startup()).unwrap();
        assert_eq!(recovered.entities.revision(), before + 2);
        assert_eq!(
            recovered
                .system_runtime
                .owner_value::<u64>(&system(), owner()),
            Some((1, 11))
        );
        drop(recovered);
        fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn owner_admission_obeys_the_shared_pending_bound_even_with_an_empty_writer_queue() {
    let path = temp_save_dir("mixed-pending-capacity");
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 1, mixed_startup()).unwrap();
    for index in 0..crate::server::durable::MAX_PENDING_DURABLE_ACTIONS {
        assert!(
            state
                .durability
                .request_epoch_grant(index as u128 + 1, TickId::new(6))
                .unwrap()
                .is_none()
        );
        let (release, receipt) = hold(&mut state, index);
        // Explicit writer completion makes this pending/apply pressure, not
        // a race with the WAL worker's bounded submission queue.
        release
            .send(receipt.recv_timeout(Duration::from_secs(5)).unwrap())
            .unwrap();
    }
    let next_id = state.durability.next_id;
    assert_eq!(
        stage_owner(&mut state).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(state.durability.next_id, next_id);
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system(), owner()),
        Some((0, 10))
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    let wave = stage_owner_attempt(&mut state, 1).unwrap().unwrap();
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(
        state.system_runtime.owner_value::<u64>(&system(), owner()),
        Some((1, 11))
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
