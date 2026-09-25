use super::*;
use crate::content::{Catalog, EntityTypeId};
use crate::server::drops;
use crate::server::entities::{
    EntityId, EntityPatch, EntityPayload, EntitySpawn, EntityTypeRegistry,
    EntityTypeRegistryBuilder, PlayerEntityPayload, decode_checkpoint, encode_checkpoint,
    register_kiln_entity_type, register_player_entity_type,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-entity-mirror-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(
    capacity: usize,
) -> (
    EntityStore,
    Arc<EntityTypeRegistry>,
    EntityCheckpointStore,
    EntityCheckpointMirror,
    TestDir,
) {
    let save = TestDir::new();
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    drops::register_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    register_player_entity_type(&mut builder).unwrap();
    register_kiln_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    let types = Arc::new(builder.freeze().unwrap());
    let live = EntityStore::new(Arc::clone(&types));
    let baseline =
        decode_checkpoint(&encode_checkpoint(&live).unwrap(), Arc::clone(&types)).unwrap();
    let checkpoint = EntityCheckpointStore::new(&save.0).unwrap();
    let mirror = EntityCheckpointMirror::start(baseline, checkpoint.clone(), capacity).unwrap();
    (live, types, checkpoint, mirror, save)
}

fn poll_checkpoint(
    mirror: &EntityCheckpointMirror,
    ticket: &mut CheckpointTicket,
) -> CheckpointReceipt {
    for _ in 0..5_000 {
        match mirror.poll_checkpoint(ticket) {
            Ok(Some(receipt)) => return receipt,
            Ok(None) => std::thread::sleep(Duration::from_millis(1)),
            Err(error) => panic!("entity mirror checkpoint failed: {error}"),
        }
    }
    panic!("entity mirror checkpoint did not finish");
}

fn spawn_player(live: &mut EntityStore, mirror: &mut EntityCheckpointMirror) -> EntityId {
    let mut permit = mirror.try_reserve_durable().unwrap().unwrap();
    let batch = live
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: EntityTypeId(2),
            position: [15.5, 96.5, 0.5],
            payload: EntityPayload::new(PlayerEntityPayload::new(1, 2, 3, 0)),
            spawn_tick: 1,
        })
        .unwrap();
    let id = batch.entity_id();
    live.apply_committed(batch.clone()).unwrap();
    permit.mark_authoritative_change().unwrap();
    mirror.submit_durable(permit, batch).unwrap();
    id
}

#[test]
fn ordered_durable_motion_transfer_and_checkpoint_recover_exact_latest_state() {
    let (mut live, types, checkpoint, mut mirror, _save) = fixture(8);
    let id = spawn_player(&mut live, &mut mirror);

    let mut motion_permit = mirror.try_reserve_motion().unwrap().unwrap();
    live.update_mobile_motion(id, 1, [15.75, 96.5, 0.5])
        .unwrap();
    motion_permit.mark_authoritative_change().unwrap();
    mirror
        .submit_motion(motion_permit, live.mobile_motion_snapshot(id).unwrap())
        .unwrap();

    let mut transfer_permit = mirror.try_reserve_durable().unwrap().unwrap();
    let transfer = live
        .prepare_transfer(
            id,
            1,
            [16.25, 96.5, 0.5],
            crate::server::entities::EntityPatch::default(),
        )
        .unwrap();
    live.apply_committed(transfer.clone()).unwrap();
    transfer_permit.mark_authoritative_change().unwrap();
    mirror.submit_durable(transfer_permit, transfer).unwrap();

    let mut motion_permit = mirror.try_reserve_motion().unwrap().unwrap();
    live.update_mobile_motion(id, 3, [16.5, 96.5, 0.5]).unwrap();
    motion_permit.mark_authoritative_change().unwrap();
    mirror
        .submit_motion(motion_permit, live.mobile_motion_snapshot(id).unwrap())
        .unwrap();

    let mut ticket = mirror.try_begin_checkpoint().unwrap().unwrap();
    assert!(mirror.metrics().fenced);
    assert!(mirror.try_reserve_motion().unwrap().is_none());
    let receipt = poll_checkpoint(&mirror, &mut ticket);
    assert_eq!(receipt.event_sequence, 4);
    assert_eq!(receipt.durable_sequence, 2);
    assert_eq!(receipt.registry_revision, live.revision());
    let restored = decode_checkpoint(&checkpoint.read().unwrap().unwrap(), types).unwrap();
    assert_eq!(restored.durable_sequence(), 2);
    assert_eq!(
        restored.mobile_motion_snapshot(id),
        live.mobile_motion_snapshot(id)
    );
    assert_eq!(restored.owner(id), live.owner(id));
    mirror.finish_checkpoint_fence(ticket).unwrap();
    assert!(!mirror.metrics().fenced);
}

#[test]
fn delayed_payload_receipt_keeps_newer_checkpoint_only_motion() {
    let (mut live, types, checkpoint, mut mirror, _save) = fixture(8);
    let id = spawn_player(&mut live, &mut mirror);
    let update = live
        .prepare_update(
            id,
            1,
            EntityPatch {
                payload: Some(EntityPayload::new(PlayerEntityPayload::new(4, 5, 6, 0))),
                ..EntityPatch::default()
            },
        )
        .unwrap();
    let mut durable_permit = mirror.try_reserve_durable().unwrap().unwrap();
    let mut motion_permit = mirror.try_reserve_motion().unwrap().unwrap();
    live.update_mobile_motion(id, 1, [15.75, 96.5, 0.5])
        .unwrap();
    motion_permit.mark_authoritative_change().unwrap();
    mirror
        .submit_motion(motion_permit, live.mobile_motion_snapshot(id).unwrap())
        .unwrap();
    live.apply_committed(update.clone()).unwrap();
    durable_permit.mark_authoritative_change().unwrap();
    mirror.submit_durable(durable_permit, update).unwrap();

    let mut ticket = mirror.try_begin_checkpoint().unwrap().unwrap();
    let receipt = poll_checkpoint(&mirror, &mut ticket);
    assert_eq!(receipt.event_sequence, 3);
    let restored = decode_checkpoint(&checkpoint.read().unwrap().unwrap(), types).unwrap();
    assert_eq!(restored.public_view(id), live.public_view(id));
    assert_eq!(
        restored.mobile_motion_snapshot(id),
        live.mobile_motion_snapshot(id)
    );
    mirror.finish_checkpoint_fence(ticket).unwrap();
}

#[test]
fn reservation_capacity_and_unresolved_wal_permit_defer_checkpoint_without_loss() {
    let (_live, _types, _checkpoint, mut mirror, _save) = fixture(1);
    let durable = mirror.try_reserve_durable().unwrap().unwrap();
    assert!(mirror.try_reserve_motion().unwrap().is_none());
    assert!(mirror.try_begin_checkpoint().unwrap().is_none());
    assert_eq!(mirror.metrics().outstanding, 1);
    assert_eq!(mirror.metrics().reserved, 1);
    drop(durable);
    assert_eq!(mirror.metrics().outstanding, 0);
    let mut ticket = mirror.try_begin_checkpoint().unwrap().unwrap();
    assert!(mirror.try_reserve_durable().unwrap().is_none());
    let receipt = poll_checkpoint(&mirror, &mut ticket);
    assert_eq!(receipt.event_sequence, 0);
    mirror.finish_checkpoint_fence(ticket).unwrap();
    let motion = mirror.try_reserve_motion().unwrap().unwrap();
    drop(motion);
    assert_eq!(mirror.metrics().high_water, 1);
}

#[test]
fn admitted_event_permit_cannot_be_dropped_silently() {
    let (_live, _types, _checkpoint, mut mirror, _save) = fixture(1);
    let mut permit = mirror.try_reserve_durable().unwrap().unwrap();
    permit.mark_authoritative_change().unwrap();
    drop(permit);
    assert!(mirror.check_health().is_err());
    assert_eq!(mirror.metrics().outstanding, 0);
    assert!(mirror.try_reserve_durable().is_err());
}

#[test]
fn malformed_ordered_event_fails_closed_without_checkpoint_publication() {
    let (_live, _types, checkpoint, mut mirror, _save) = fixture(2);
    let mut permit = mirror.try_reserve_motion().unwrap().unwrap();
    permit.mark_authoritative_change().unwrap();
    mirror
        .submit_motion(
            permit,
            EntityMotionSnapshot {
                id: EntityId::new(1).unwrap(),
                revision: 2,
                position: [1.0, 2.0, 3.0],
            },
        )
        .unwrap();
    for _ in 0..5_000 {
        if mirror.check_health().is_err() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(mirror.check_health().is_err());
    assert!(mirror.try_reserve_durable().is_err());
    assert!(checkpoint.read().unwrap().is_none());
    assert!(mirror.metrics().failed);
}

#[test]
fn checkpoint_io_failure_closes_admission_and_reports_error() {
    let (_live, _types, checkpoint, mut mirror, save) = fixture(2);
    fs::write(save.0.join("entities/.entities.bin.tmp"), b"orphan").unwrap();
    let mut ticket = mirror.try_begin_checkpoint().unwrap().unwrap();
    for _ in 0..5_000 {
        match mirror.poll_checkpoint(&mut ticket) {
            Ok(None) => std::thread::sleep(Duration::from_millis(1)),
            Ok(Some(_)) => panic!("orphan temporary checkpoint was accepted"),
            Err(_) => break,
        }
    }
    assert!(mirror.check_health().is_err());
    assert!(mirror.try_reserve_motion().is_err());
    assert!(checkpoint.read().is_err());
}
