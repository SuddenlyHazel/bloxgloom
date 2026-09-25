use super::*;
use crate::content::{Catalog, EntityTypeDef, EntityTypeId};
use crate::server::entities::{
    EntityCodecError, EntityLocation, EntityOwnership, EntityPayload, EntityPayloadCodec,
    EntitySpawn, EntityTypeRegistration, EntityTypeRegistryBuilder, TickPolicy,
};
use crate::server::journal::Transaction;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT_DIR: AtomicU64 = AtomicU64::new(1);
const MOBILE_TYPE: EntityTypeId = EntityTypeId(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-entity-recovery-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn journal(&self) -> Journal {
        Journal::open(self.0.join("server.wal")).unwrap()
    }

    fn append(&self, id: u128, change: &crate::server::entities::PreparedEntityTransaction) {
        let writer = self.journal().into_writer(8, Duration::ZERO).unwrap();
        let receiver = writer
            .try_submit(Transaction::new(id, id as u64, change.changes().to_vec()))
            .unwrap();
        receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        writer.shutdown().unwrap();
    }

    fn recover(&self, types: Arc<EntityTypeRegistry>) -> io::Result<PreparedEntityRecovery> {
        let journal = self.journal();
        prepare(&self.0, &journal, &journal.latest_values(), types)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ByteCodec;

impl EntityPayloadCodec for ByteCodec {
    fn decode(&self, bytes: &[u8]) -> Result<EntityPayload, EntityCodecError> {
        let [value] = bytes else {
            return Err(EntityCodecError::InvalidData);
        };
        Ok(EntityPayload::new(*value))
    }

    fn encode(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        payload
            .downcast_ref::<u8>()
            .copied()
            .map(|value| vec![value])
            .ok_or(EntityCodecError::InvalidData)
    }

    fn public_view(&self, payload: &EntityPayload) -> Result<Vec<u8>, EntityCodecError> {
        self.encode(payload)
    }
}

fn fixture() -> (Arc<EntityTypeRegistry>, EntityStore) {
    let mut catalog = Catalog::new();
    catalog
        .register_entity_type(EntityTypeDef {
            id: MOBILE_TYPE,
            key: "test:mobile".into(),
            schema_version: 1,
            schema_fingerprint: 71,
        })
        .unwrap();
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    builder
        .register(EntityTypeRegistration {
            id: MOBILE_TYPE,
            ownership: EntityOwnership::Mobile,
            tick_policy: TickPolicy::Never,
            max_payload_bytes: 1,
            codec: Arc::new(ByteCodec),
        })
        .unwrap();
    let types = Arc::new(builder.freeze().unwrap());
    let store = EntityStore::new(types.clone());
    (types, store)
}

fn spawn(store: &EntityStore) -> crate::server::entities::PreparedEntityTransaction {
    store
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: MOBILE_TYPE,
            position: [1.0, 1.0, 1.0],
            payload: EntityPayload::new(7u8),
            spawn_tick: 1,
        })
        .unwrap()
}

fn position(store: &EntityStore, id: EntityId) -> [f32; 3] {
    let snapshot = store.snapshot(id).unwrap();
    let EntityLocation::Mobile { position } = snapshot.location else {
        panic!("expected mobile entity");
    };
    position
}

#[test]
fn checkpointed_motion_ahead_of_wal_fence_survives_recovery() {
    let dir = TestDir::new();
    let (types, mut live) = fixture();
    let spawn = spawn(&live);
    let id = spawn.entity_id();
    dir.append(1, &spawn);
    live.apply_committed(spawn).unwrap();
    live.update_mobile_motion(id, 1, [2.0, 1.0, 1.0]).unwrap();
    EntityCheckpointStore::new(&dir.0)
        .unwrap()
        .write(&encode_checkpoint(&live).unwrap())
        .unwrap();

    let recovered = dir.recover(types).unwrap();
    assert_eq!(position(&recovered.entities, id), [2.0, 1.0, 1.0]);
    assert!(recovered.replay.is_none());
}

#[test]
fn lagging_checkpoint_replays_later_wal_transfer() {
    let dir = TestDir::new();
    let (types, mut live) = fixture();
    let spawn = spawn(&live);
    let id = spawn.entity_id();
    dir.append(1, &spawn);
    live.apply_committed(spawn).unwrap();
    EntityCheckpointStore::new(&dir.0)
        .unwrap()
        .write(&encode_checkpoint(&live).unwrap())
        .unwrap();
    let transfer = live
        .prepare_transfer(
            id,
            1,
            [17.0, 1.0, 1.0],
            crate::server::entities::EntityPatch::default(),
        )
        .unwrap();
    dir.append(2, &transfer);

    let recovered = dir.recover(types.clone()).unwrap();
    assert_eq!(position(&recovered.entities, id), [17.0, 1.0, 1.0]);
    assert!(recovered.replay.is_some());
    recovered.publish_replay().unwrap();
    assert!(dir.recover(types).unwrap().replay.is_none());
}

#[test]
fn same_revision_conflicting_checkpoint_motion_fails_closed() {
    let dir = TestDir::new();
    let (types, mut live) = fixture();
    let spawn = spawn(&live);
    let id = spawn.entity_id();
    dir.append(1, &spawn);
    live.apply_committed(spawn).unwrap();
    let mut divergent =
        decode_checkpoint(&encode_checkpoint(&live).unwrap(), types.clone()).unwrap();
    divergent
        .update_mobile_motion(id, 1, [2.0, 1.0, 1.0])
        .unwrap();
    EntityCheckpointStore::new(&dir.0)
        .unwrap()
        .write(&encode_checkpoint(&divergent).unwrap())
        .unwrap();
    let transfer = live
        .prepare_transfer(
            id,
            1,
            [17.0, 1.0, 1.0],
            crate::server::entities::EntityPatch::default(),
        )
        .unwrap();
    dir.append(2, &transfer);

    let error = match dir.recover(types) {
        Ok(_) => panic!("same motion revision with different position was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn one_wal_record_recovers_linked_block_and_entity_after_unapplied_receipt() {
    use crate::inventory::Stack;
    use crate::items::ItemId;
    use crate::server::drops::DropEntityPayload;
    use crate::server::durable::CommitAction;
    use crate::server::server_state;
    use crate::server::simulation::TickId;
    use crate::world::GLOWSTONE;

    let dir = TestDir::new();
    let mut live = server_state(71, dir.0.clone()).unwrap();
    let edits = live.world.prepare_edits(&[(8, 96, 8, GLOWSTONE)]).unwrap();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].changed);
    let spawn = live
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: EntityTypeId(1),
            position: [8.5, 97.0, 8.5],
            payload: DropEntityPayload::new(
                Stack::new(ItemId::new(4), 3),
                1,
                Duration::from_millis(100),
            )
            .into_entity_payload(),
            spawn_tick: 1,
        })
        .unwrap();
    let id = spawn.entity_id();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entity_wakes: Vec::new(),
        entities: Some(spawn),
    };
    let sequence = live.durability.writer.sequence();
    let entity_permit = live
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    assert!(
        live.durability
            .try_stage(TickId::new(1), &action, Some(entity_permit))
            .unwrap()
    );
    live.durability.pending[0]
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(live.durability.writer.sequence(), sequence + 1);
    // Simulate a crash after WAL sync but before either participant applies.
    drop(live);

    let journal = dir.journal();
    let linked = journal.records().last().unwrap();
    assert!(
        linked
            .changes
            .iter()
            .any(|change| change.key.domain == "bloxgloom:chunk_snapshot")
    );
    assert!(
        linked
            .changes
            .iter()
            .any(|change| change.key.domain == ENTITY_RECORD_DOMAIN)
    );
    drop(journal);
    let recovered = server_state(71, dir.0.clone()).unwrap();
    assert_eq!(recovered.world.cached_block(8, 96, 8), Some(GLOWSTONE));
    assert_eq!(position(&recovered.entities, id), [8.5, 97.0, 8.5]);
    assert_eq!(recovered.entities.len(), 1);
}

#[test]
fn synced_entity_action_reaches_checkpoint_mirror_before_restart() {
    use crate::inventory::Stack;
    use crate::items::ItemId;
    use crate::server::drops::DropEntityPayload;
    use crate::server::durable::{CommitAction, receipt::poll_journal_receipts};
    use crate::server::server_state;
    use crate::server::simulation::TickId;
    use std::time::Instant;

    let dir = TestDir::new();
    let mut live = server_state(72, dir.0.clone()).unwrap();
    let spawn = live
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: EntityTypeId(1),
            position: [8.5, 97.0, 8.5],
            payload: DropEntityPayload::new(
                Stack::new(ItemId::new(4), 3),
                1,
                Duration::from_millis(100),
            )
            .into_entity_payload(),
            spawn_tick: 1,
        })
        .unwrap();
    let id = spawn.entity_id();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entity_wakes: Vec::new(),
        entities: Some(spawn),
    };
    let permit = live
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    assert!(
        live.durability
            .try_stage(TickId::new(1), &action, Some(permit))
            .unwrap()
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while !live.durability.pending.is_empty() {
        poll_journal_receipts(&mut live).unwrap();
        assert!(Instant::now() < deadline, "WAL receipt did not arrive");
        std::thread::yield_now();
    }
    assert_eq!(live.entities.len(), 1);
    let mut ticket = live
        .durability
        .entity_mirror
        .try_begin_checkpoint()
        .unwrap()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let checkpoint = loop {
        if let Some(receipt) = live
            .durability
            .entity_mirror
            .poll_checkpoint(&mut ticket)
            .unwrap()
        {
            break receipt;
        }
        assert!(
            Instant::now() < deadline,
            "entity checkpoint did not finish"
        );
        std::thread::yield_now();
    };
    assert_eq!(
        checkpoint.durable_sequence,
        live.entities.durable_sequence()
    );
    assert_eq!(checkpoint.registry_revision, live.entities.revision());
    live.durability
        .entity_mirror
        .finish_checkpoint_fence(ticket)
        .unwrap();
    drop(live);

    let recovered = server_state(72, dir.0.clone()).unwrap();
    assert_eq!(recovered.entities.len(), 1);
    assert_eq!(position(&recovered.entities, id), [8.5, 97.0, 8.5]);
}
