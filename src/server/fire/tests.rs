use super::codec::{key_bytes, read_key};
use super::scheduler::{FireCursor, cursor_key, frontier_key, mailbox_key, owner_lane};
use super::*;
use crate::lighting::{LightField, LightSample};
use crate::server::journal::StateKey;
use crate::server::simulation::TickId;
use crate::world::{
    AIR, BlockId, CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkKey, GLOWSTONE, STONE, WOOD, World,
    world_to_chunk,
};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-fire-{}-{stamp}-{sequence}",
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

fn chunk(x: i32, y: i32, z: i32) -> ChunkKey {
    ChunkKey { x, y, z }
}

#[test]
fn benchmark_frontier_bootstrap_precedes_first_live_fire_tick() {
    let save = TestDir::new();
    let mut world = World::with_capacity(55, save.0.clone(), 8).unwrap();
    let owner = chunk(0, 4, 0);
    world.get_chunk(owner).unwrap();
    let edits = world.prepare_edits(&[(0, 65, 0, WOOD)]).unwrap();
    world.apply_prepared_edits(edits).unwrap();
    let mut fire = FireRuntime::new(FireRecovered::default(), 2).unwrap();
    let bootstrap = BTreeMap::from([(owner, vec![256])]);
    let wave = fire
        .prepare_benchmark_frontier_wave(&bootstrap, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    let transaction = wave.transactions.into_iter().next().unwrap();
    assert!(
        transaction
            .changes()
            .iter()
            .all(|change| change.before != change.after)
    );
    fire.mark_submitted(&transaction).unwrap();
    fire.install_synced(transaction).unwrap();

    // The first production tick must be strictly later than the fixture
    // cursor. Reusing tick 1 could emit an identical cursor WAL transition.
    let live = fire
        .prepare_source_wave(
            &mut world,
            &crate::server::builtins::builtin_phase_plan().unwrap(),
            TickId::new(2),
        )
        .unwrap();
    assert_eq!(live.transactions.len(), 1);
    assert!(
        live.transactions[0]
            .changes()
            .iter()
            .all(|change| change.before != change.after)
    );
}

#[test]
fn durable_lane_age_prioritizes_an_owner_deferred_by_wal_pressure() {
    let recent = chunk(0, 4, 0);
    let deferred = (1..32)
        .map(|x| chunk(x, 4, 0))
        .find(|owner| owner_lane(*owner) != owner_lane(recent))
        .unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(
            &cursor_key(owner_lane(recent)),
            &FireCursor {
                last_owner: Some(recent),
                last_source: None,
                last_tick: 9,
            }
            .encode(),
        )
        .unwrap();
    let fire = FireRuntime::new(recovered, 1).unwrap();
    let mut transactions = fire
        .prepare_benchmark_frontier_wave(
            &BTreeMap::from([(recent, vec![256]), (deferred, vec![256])]),
            TickId::new(10),
        )
        .unwrap()
        .transactions;
    assert_eq!(transactions.len(), 2);
    fire.prioritize_transactions(&mut transactions);
    assert_eq!(transactions[0].owner(), deferred);
}

fn owner_apply_hash(workers: usize) -> u64 {
    let save = TestDir::new();
    let mut world = World::with_capacity(55, save.0.clone(), 8).unwrap();
    let cells = [(15, 200, 0), (16, 200, 0), (-1, 200, 0), (0, 200, 16)];
    let mut keys: Vec<_> = cells
        .iter()
        .map(|&(x, y, z)| world_to_chunk(x, y, z).0)
        .collect();
    keys.sort_unstable_by_key(|key| (key.x, key.y, key.z));
    for &key in &keys {
        world.get_chunk(key).unwrap();
    }
    let edits: Vec<_> = cells.iter().map(|&(x, y, z)| (x, y, z, WOOD)).collect();
    let prepared = world.prepare_edits(&edits).unwrap();
    let mut fire = FireRuntime::new(FireRecovered::default(), workers).unwrap();
    fire.apply_synced_world_edits(&mut world, prepared).unwrap();
    for &(x, y, z) in &cells {
        assert_eq!(world.cached_block(x, y, z), Some(WOOD));
    }
    assert_eq!(world.pending_snapshot_count(), keys.len());

    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for key in keys {
        let chunk = world.cached_arc_chunk(key).unwrap();
        for byte in chunk.version.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
        for cell in 0..CHUNK_VOLUME {
            for byte in chunk.block_index(cell).unwrap().0.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    hash
}

#[test]
fn post_wal_owner_workers_match_one_vs_four_across_chunk_boundaries() {
    assert_eq!(owner_apply_hash(1), owner_apply_hash(4));
}

#[test]
fn stale_post_wal_batch_rejects_all_owners_before_worker_mutation() {
    let save = TestDir::new();
    let mut world = World::with_capacity(56, save.0.clone(), 4).unwrap();
    let first = (15, 200, 0);
    let second = (16, 200, 0);
    for &(x, y, z) in &[first, second] {
        world.get_chunk(world_to_chunk(x, y, z).0).unwrap();
    }
    let first_before = world.cached_block(first.0, first.1, first.2).unwrap();
    let stale = world
        .prepare_edits(&[
            (first.0, first.1, first.2, WOOD),
            (second.0, second.1, second.2, WOOD),
        ])
        .unwrap();
    let newer = world
        .prepare_edits(&[(second.0, second.1, second.2, STONE)])
        .unwrap();
    world.apply_prepared_edits(newer).unwrap();
    let first_version = world.cached_version(world_to_chunk(first.0, first.1, first.2).0);
    let mut fire = FireRuntime::new(FireRecovered::default(), 4).unwrap();
    let error = fire
        .apply_synced_world_edits(&mut world, stale)
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(
        world.cached_version(world_to_chunk(first.0, first.1, first.2).0),
        first_version
    );
    assert_eq!(
        world.cached_block(first.0, first.1, first.2),
        Some(first_before)
    );
    assert_eq!(
        world.cached_block(second.0, second.1, second.2),
        Some(STONE)
    );
}

#[test]
fn post_wal_batch_larger_than_cache_fails_before_any_owner_swap() {
    let save = TestDir::new();
    let mut world = World::with_capacity(57, save.0.clone(), 2).unwrap();
    let mut edits = Vec::new();
    for x in [0, 16, 32] {
        let key = world_to_chunk(x, 200, 0).0;
        world.get_chunk(key).unwrap();
        edits.extend(world.prepare_edits(&[(x, 200, 0, WOOD)]).unwrap());
    }
    let mut fire = FireRuntime::new(FireRecovered::default(), 4).unwrap();
    let error = fire
        .apply_synced_world_edits(&mut world, edits)
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(world.pending_snapshot_count(), 0);
    for x in [16, 32] {
        assert_ne!(world.cached_block(x, 200, 0), Some(WOOD));
    }
}

#[test]
fn frontier_due_order_keeps_original_tick_and_rejects_corruption() {
    let mut frontier = FireFrontier::default();
    frontier.insert(3, 40).unwrap();
    frontier.insert(7, 30).unwrap();
    frontier.insert(3, 20).unwrap();
    frontier.insert(3, 50).unwrap();
    assert_eq!(frontier.due(25, 32), vec![(3, 20)]);
    assert_eq!(frontier.due(40, 32), vec![(3, 20), (7, 30)]);
    let bytes = frontier.encode();
    assert_eq!(FireFrontier::decode(&bytes).unwrap(), frontier);
    let mut corrupt = bytes.clone();
    corrupt[9] ^= 0x80;
    assert!(FireFrontier::decode(&corrupt).is_err());
    assert!(frontier.insert(4_096, 1).is_err());
    assert!(frontier.insert(4, 0).is_err());
}

#[test]
fn source_scoped_mailboxes_round_trip_and_reject_identity_collision() {
    let source = chunk(-3, 2, 1);
    let destination = chunk(-4, 2, 1);
    let key = mailbox_key(destination, source);
    assert_eq!(
        super::pending::decode_pending_key(&key.bytes).unwrap(),
        (destination, source)
    );
    assert_eq!(read_key(&key_bytes(source)).unwrap(), source);

    let ignition = FireIgnition {
        id: FireIgnitionId {
            source_tick: 17,
            source_cell: 256,
            direction: 0,
        },
        target_cell: 271,
        activate_at: 18,
    };
    let mut pending = FirePending::default();
    assert!(pending.insert(ignition).unwrap());
    assert!(!pending.insert(ignition).unwrap());
    assert!(
        pending
            .insert(FireIgnition {
                target_cell: 15,
                ..ignition
            })
            .is_err()
    );
    let bytes = pending.encode();
    assert_eq!(FirePending::decode(&bytes).unwrap(), pending);
    let mut corrupt = bytes;
    corrupt[12] ^= 1;
    assert!(FirePending::decode(&corrupt).is_err());
}

#[test]
fn recovered_fire_values_validate_domains_and_cursor_lanes() {
    let source = chunk(-3, 2, 1);
    let mut frontier = FireFrontier::default();
    frontier.insert(25, 19).unwrap();
    let mut restored = FireRecovered::default();
    assert!(
        restored
            .apply_value(&frontier_key(source), &frontier.encode())
            .unwrap()
    );
    let lane = owner_lane(source);
    let cursor = scheduler::FireCursor {
        last_owner: Some(source),
        last_source: None,
        last_tick: 20,
    };
    assert!(
        restored
            .apply_value(&cursor_key(lane), &cursor.encode())
            .unwrap()
    );
    assert_eq!(restored.last_tick(), 20);
    let mut wrong_lane = (lane + 1) % scheduler::FIRE_LANES;
    if wrong_lane == lane {
        wrong_lane = (wrong_lane + 1) % scheduler::FIRE_LANES;
    }
    assert!(
        restored
            .apply_value(&cursor_key(wrong_lane), &cursor.encode())
            .is_err()
    );
    assert!(
        !restored
            .apply_value(&StateKey::new("mod:other", vec![]), b"opaque")
            .unwrap()
    );
    let values = restored.checkpoint_values();
    assert!(values.iter().any(|(key, _)| key == &frontier_key(source)));
    assert!(values.iter().any(|(key, _)| key == &cursor_key(lane)));
}

#[test]
fn checkpoint_store_rejects_orphans_and_corruption_and_cleans_interrupted_temp() {
    let directory = TestDir::new();
    let store = FireCheckpointStore::new(&directory.0).unwrap();
    let key = cursor_key(0);
    let value = scheduler::FireCursor {
        last_owner: None,
        last_source: None,
        last_tick: 40,
    }
    .encode();
    store.write(&key, &value).unwrap();
    assert_eq!(store.read(&key).unwrap(), Some(value.clone()));
    let latest = BTreeMap::from([(key.clone(), value.clone())]);
    store.validate_no_orphans(&latest).unwrap();
    assert!(store.validate_no_orphans(&BTreeMap::new()).is_err());

    let temp = directory.0.join("fire/.cursor_00.fire.1.1.tmp");
    fs::write(&temp, b"interrupted partial write").unwrap();
    store.validate_no_orphans(&latest).unwrap();
    store.cleanup_interrupted_temps().unwrap();
    assert!(!temp.exists());

    let path = directory.0.join("fire/checkpoints.fire");
    let mut corrupt = fs::read(&path).unwrap();
    corrupt[8] ^= 0x80;
    fs::write(&path, corrupt).unwrap();
    assert!(FireCheckpointStore::new(&directory.0).is_err());
    store.write(&key, &value).unwrap();
    store.write(&key, &[]).unwrap();
    assert!(store.read(&key).unwrap().is_none());
    assert!(!directory.0.join("fire/cursor_00.fire").exists());
}

#[test]
fn checkpoint_batch_replaces_one_complete_snapshot_and_applies_tombstones() {
    let directory = TestDir::new();
    let store = FireCheckpointStore::new(&directory.0).unwrap();
    let first = cursor_key(0);
    let second = cursor_key(1);
    let first_value = scheduler::FireCursor {
        last_owner: None,
        last_source: None,
        last_tick: 11,
    }
    .encode();
    let second_value = scheduler::FireCursor {
        last_owner: None,
        last_source: None,
        last_tick: 12,
    }
    .encode();
    store
        .write_batch(&[
            (first.clone(), first_value.clone()),
            (second.clone(), second_value.clone()),
        ])
        .unwrap();
    assert!(!directory.0.join("fire/cursor_00.fire").exists());
    assert!(!directory.0.join("fire/cursor_01.fire").exists());

    let reopened = FireCheckpointStore::new(&directory.0).unwrap();
    assert_eq!(reopened.read(&first).unwrap(), Some(first_value));
    assert_eq!(reopened.read(&second).unwrap(), Some(second_value.clone()));
    let next_value = scheduler::FireCursor {
        last_owner: None,
        last_source: None,
        last_tick: 13,
    }
    .encode();
    reopened
        .write_batch(&[
            (first.clone(), Vec::new()),
            (second.clone(), next_value.clone()),
        ])
        .unwrap();

    let recovered = FireCheckpointStore::new(&directory.0).unwrap();
    assert_eq!(recovered.read(&first).unwrap(), None);
    assert_eq!(recovered.read(&second).unwrap(), Some(next_value.clone()));
    recovered
        .validate_no_orphans(&BTreeMap::from([(second.clone(), next_value)]))
        .unwrap();
    assert!(recovered.validate_no_orphans(&BTreeMap::new()).is_err());
}

#[test]
fn first_aggregate_write_preserves_legacy_per_key_checkpoints() {
    let directory = TestDir::new();
    let old_key = cursor_key(2);
    let new_key = cursor_key(3);
    let old_value = scheduler::FireCursor {
        last_owner: None,
        last_source: None,
        last_tick: 21,
    }
    .encode();
    let new_value = scheduler::FireCursor {
        last_owner: None,
        last_source: None,
        last_tick: 22,
    }
    .encode();
    let fire_dir = directory.0.join("fire");
    fs::create_dir_all(&fire_dir).unwrap();
    fs::write(
        fire_dir.join("cursor_02.fire"),
        super::checkpoint::encode_envelope(&old_key, &old_value).unwrap(),
    )
    .unwrap();

    let store = FireCheckpointStore::new(&directory.0).unwrap();
    assert_eq!(store.read(&old_key).unwrap(), Some(old_value.clone()));
    store
        .write_batch(&[(new_key.clone(), new_value.clone())])
        .unwrap();
    let recovered = FireCheckpointStore::new(&directory.0).unwrap();
    assert_eq!(recovered.read(&old_key).unwrap(), Some(old_value.clone()));
    assert_eq!(recovered.read(&new_key).unwrap(), Some(new_value.clone()));
    recovered
        .validate_no_orphans(&BTreeMap::from([
            (old_key, old_value),
            (new_key, new_value),
        ]))
        .unwrap();
}

#[test]
fn seed_mailbox_is_invisible_until_synced_and_survives_value_recovery() {
    let source = chunk(0, 4, 0);
    let mut runtime = FireRuntime::new(FireRecovered::default(), 1).unwrap();
    let seed = runtime
        .prepare_seed_from_edit(TickId::new(7), source, 0, GLOWSTONE)
        .unwrap()
        .unwrap();
    assert_eq!(seed.changes().len(), 4);
    assert!(runtime.snapshot().checkpoint_values().is_empty());
    runtime.mark_seed_submitted(&seed).unwrap();
    assert!(
        runtime
            .prepare_seed_from_edit(TickId::new(8), source, 0, GLOWSTONE)
            .is_err()
    );
    runtime.install_seed_synced(seed.clone()).unwrap();
    assert_eq!(runtime.pending_destinations().count(), 4);

    let mut recovered = FireRecovered::default();
    for change in seed.changes() {
        recovered.apply_value(&change.key, &change.after).unwrap();
    }
    assert_eq!(recovered.last_tick(), 8);
    assert_eq!(recovered.checkpoint_values().len(), 4);
    let restarted = FireRuntime::new(recovered, 1).unwrap();
    assert_eq!(restarted.pending_destinations().count(), 4);
}

#[test]
fn worker_burn_and_delivery_are_separate_wal_plans() {
    let directory = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(23, directory.0.clone(), 64).unwrap();
    world.get_chunk(owner).unwrap();
    let prepared = world
        .prepare_edits(&[(1, 65, 1, WOOD), (2, 65, 1, WOOD)])
        .unwrap();
    world.apply_prepared_edits(prepared).unwrap();
    let mut frontier = FireFrontier::default();
    frontier.insert(273, 1).unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    let mut runtime = FireRuntime::new(recovered, 2).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();

    let wave = runtime
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    let source = wave.transactions.into_iter().next().unwrap();
    assert_eq!(source.burns(), &[273]);
    assert!(source.world_edit.as_ref().is_some_and(|edit| edit.changed));
    assert!(
        source
            .changes()
            .iter()
            .any(|change| change.key.domain == "bloxgloom:chunk_snapshot")
    );
    // A prepared plan has not yet consumed the persisted source frontier.
    assert!(
        runtime
            .snapshot()
            .checkpoint_values()
            .iter()
            .any(|(key, _)| key == &frontier_key(owner))
    );
    runtime.mark_submitted(&source).unwrap();
    world
        .apply_prepared_edits(vec![source.world_edit.clone().unwrap()])
        .unwrap();
    runtime.install_synced(source).unwrap();

    let delivery = runtime
        .prepare_delivery_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    let local = delivery
        .transactions
        .into_iter()
        .find(|transaction| transaction.owner() == owner)
        .unwrap();
    assert!(local.burns().is_empty());
    assert!(local.world_edit.is_none());
    assert!(
        local
            .changes()
            .iter()
            .all(|change| change.key.domain != "bloxgloom:chunk_snapshot")
    );
    runtime.mark_submitted(&local).unwrap();
    runtime.install_synced(local).unwrap();

    let none = runtime
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert!(none.transactions.is_empty());
    let next = runtime
        .prepare_source_wave(&mut world, &plan, TickId::new(2))
        .unwrap();
    assert_eq!(next.transactions.len(), 1);
    assert_eq!(next.transactions[0].burns(), &[274]);
}

#[test]
fn full_destination_mailbox_defers_source_without_consuming_frontier() {
    let directory = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(29, directory.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    let edits = world.prepare_edits(&[(1, 65, 1, WOOD)]).unwrap();
    world.apply_prepared_edits(edits).unwrap();
    let mut frontier = FireFrontier::default();
    frontier.insert(273, 1).unwrap();
    // A full frontier must be allowed to drain despite pending delivery;
    // otherwise it could deadlock when the destination mailbox is full too.
    for cell in 0..4_096 {
        frontier.insert(cell, 2).unwrap();
    }
    let mut mailbox = FirePending::default();
    for source_tick in 1..=super::pending::MAX_PENDING_IGNITIONS as u64 {
        mailbox
            .insert(FireIgnition {
                id: FireIgnitionId {
                    source_tick,
                    source_cell: 0,
                    direction: 0,
                },
                target_cell: 1,
                activate_at: source_tick + 1,
            })
            .unwrap();
    }
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    recovered
        .apply_value(&mailbox_key(owner, owner), &mailbox.encode())
        .unwrap();
    let mut runtime = FireRuntime::new(recovered, 2).unwrap();
    let before = runtime.snapshot().checkpoint_values();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();
    let wave = runtime
        .prepare_source_wave(&mut world, &plan, TickId::new(4_097))
        .unwrap();
    assert!(wave.transactions.is_empty());
    assert_eq!(wave.deferred_owners, 1);
    assert_eq!(runtime.snapshot().checkpoint_values(), before);
    assert_eq!(world.cached_block(1, 65, 1), Some(WOOD));
}

#[test]
fn pending_delivery_precedes_continuously_due_source_without_same_tick_ignition() {
    let directory = TestDir::new();
    let owner = chunk(0, 4, 0);
    let source = chunk(1, 4, 0);
    let mut world = World::with_capacity(41, directory.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    let edits = world
        .prepare_edits(&[(1, 65, 1, WOOD), (2, 65, 1, WOOD)])
        .unwrap();
    world.apply_prepared_edits(edits).unwrap();
    let mut frontier = FireFrontier::default();
    frontier.insert(273, 1).unwrap();
    let mut mailbox = FirePending::default();
    mailbox
        .insert(FireIgnition {
            id: FireIgnitionId {
                source_tick: 1,
                source_cell: 0,
                direction: 0,
            },
            target_cell: 274,
            activate_at: 2,
        })
        .unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    recovered
        .apply_value(&mailbox_key(owner, source), &mailbox.encode())
        .unwrap();
    let mut runtime = FireRuntime::new(recovered, 2).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();

    let source_wave = runtime
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert!(source_wave.transactions.is_empty());
    let delivery = runtime
        .prepare_delivery_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(delivery.transactions.len(), 1);
    let delivery = delivery.transactions.into_iter().next().unwrap();
    assert_eq!(delivery.delivered_effects, 1);
    runtime.mark_submitted(&delivery).unwrap();
    runtime.install_synced(delivery).unwrap();
    assert!(runtime.pending_destinations().next().is_none());
    let committed = runtime
        .snapshot()
        .checkpoint_values()
        .into_iter()
        .find(|(key, _)| key == &frontier_key(owner))
        .map(|(_, value)| FireFrontier::decode(&value).unwrap())
        .unwrap();
    assert_eq!(committed.due(1, 32), vec![(273, 1)]);
    assert_eq!(committed.due(2, 32), vec![(273, 1), (274, 2)]);
    let source_wave = runtime
        .prepare_source_wave(&mut world, &plan, TickId::new(2))
        .unwrap();
    assert_eq!(source_wave.transactions.len(), 1);
    assert_eq!(source_wave.transactions[0].burns(), &[273, 274]);
}

#[test]
fn nonflammable_delivery_omits_unchanged_frontier_wal_change() {
    let directory = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(43, directory.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    let edits = world.prepare_edits(&[(1, 65, 1, STONE)]).unwrap();
    world.apply_prepared_edits(edits).unwrap();
    let mut mailbox = FirePending::default();
    mailbox
        .insert(FireIgnition {
            id: FireIgnitionId {
                source_tick: 1,
                source_cell: 0,
                direction: 0,
            },
            target_cell: 273,
            activate_at: 2,
        })
        .unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&mailbox_key(owner, owner), &mailbox.encode())
        .unwrap();
    let mut runtime = FireRuntime::new(recovered, 1).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();
    let wave = runtime
        .prepare_delivery_wave(&mut world, &plan, TickId::new(2))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    let transaction = &wave.transactions[0];
    assert_eq!(transaction.delivered_effects, 1);
    assert!(
        transaction
            .changes()
            .iter()
            .all(|change| change.before != change.after)
    );
    assert!(
        transaction
            .changes()
            .iter()
            .all(|change| change.key.domain != "bloxgloom:fire_frontier")
    );
}

#[test]
fn one_and_four_workers_prepare_identical_multi_owner_wal_values() {
    let directory = TestDir::new();
    let mut world = World::with_capacity(31, directory.0.clone(), 64).unwrap();
    let mut cells = Vec::new();
    let mut recovered = FireRecovered::default();
    for x in 0..32 {
        let owner = chunk(x, 4, 0);
        world.get_chunk(owner).unwrap();
        cells.push((x * 16 + 1, 65, 1, WOOD));
        let mut frontier = FireFrontier::default();
        frontier.insert(273, 1).unwrap();
        recovered
            .apply_value(&frontier_key(owner), &frontier.encode())
            .unwrap();
    }
    let edits = world.prepare_edits(&cells).unwrap();
    world.apply_prepared_edits(edits).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();
    let mut serial = FireRuntime::new(recovered.clone(), 1).unwrap();
    let mut parallel = FireRuntime::new(recovered, 4).unwrap();
    let first = serial
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    let second = parallel
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    let canonical = |wave: FireWave| {
        wave.transactions
            .into_iter()
            .map(|transaction| (transaction.owner(), transaction.changes().to_vec()))
            .collect::<Vec<_>>()
    };
    let serial = canonical(first);
    let parallel = canonical(second);
    assert!(
        serial.len() >= 8,
        "multi-owner fixture must exercise workers"
    );
    assert_eq!(serial, parallel);
}

#[test]
fn cpu_fixture_hashes_match_for_128_active_chunks_across_worker_counts() {
    let report = benchmark_cpu(2, 4).unwrap();
    assert_eq!(report.active_chunks, 128);
    assert!(report.outputs_match);
    assert_eq!(
        report.single_worker.source.owner_jobs,
        report.comparison.source.owner_jobs
    );
    assert_eq!(report.single_worker.source.owner_jobs, 64);
    assert_eq!(
        report.single_worker.source.burned_cells,
        report.comparison.source.burned_cells
    );
    assert_eq!(
        report.single_worker.source.effects,
        report.comparison.source.effects
    );
    assert_eq!(
        report.single_worker.delivery.effects,
        report.comparison.delivery.effects
    );
    assert!(report.single_worker.source.owner_jobs >= 16);
    assert!(report.single_worker.source.effects > 0);
    assert!(report.single_worker.delivery.effects > 0);
    assert!(report.post_wal_apply_included);
    assert!(report.single_worker.post_wal_apply.total() > std::time::Duration::ZERO);
    assert!(report.comparison.post_wal_apply.total() > std::time::Duration::ZERO);
}

// ---------------------------------------------------------------------------
// Behaviour pins for the owner-path migration.
//
// These tests pin the OBSERVABLE outcomes of the current fire implementation
// without changing any behaviour. After the migration onto the generic owner
// / effect / domain path they must pass UNMODIFIED: any failure is a
// behaviour change, not a stale test.
// ---------------------------------------------------------------------------

/// Applies block edits to the authoritative world cache.
fn pin_apply(world: &mut World, edits: &[(i32, i32, i32, BlockId)]) {
    let prepared = world.prepare_edits(edits).unwrap();
    world.apply_prepared_edits(prepared).unwrap();
}

/// Commits one prepared fire transaction the way the coordinator does: admit
/// to the WAL, apply the synced world edit, then install the receipt.
fn pin_commit(runtime: &mut FireRuntime, world: &mut World, transaction: FireTransaction) {
    runtime.mark_submitted(&transaction).unwrap();
    if let Some(edit) = transaction.world_edit.clone() {
        world.apply_prepared_edits(vec![edit]).unwrap();
    }
    runtime.install_synced(transaction).unwrap();
}

/// Runs one full fire tick in production order (source in Simulation,
/// then delivery in InteractionCommit: pending mailboxes gate the NEXT
/// tick's source admission) and commits every transaction. Returns
/// (delivery transactions, source transactions, burned cells).
fn pin_tick(runtime: &mut FireRuntime, world: &mut World, tick: u64) -> (usize, usize, usize) {
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();
    let source = runtime
        .prepare_source_wave(world, &plan, TickId::new(tick))
        .unwrap();
    let sources = source.transactions.len();
    let burned = source
        .transactions
        .iter()
        .map(|transaction| transaction.burns().len())
        .sum();
    for transaction in source.transactions {
        pin_commit(runtime, world, transaction);
    }
    let delivery = runtime
        .prepare_delivery_wave(world, &plan, TickId::new(tick))
        .unwrap();
    let delivered = delivery.transactions.len();
    for transaction in delivery.transactions {
        pin_commit(runtime, world, transaction);
    }
    (delivered, sources, burned)
}

fn pin_recovered(values: &[(StateKey, Vec<u8>)]) -> FireRecovered {
    let mut recovered = FireRecovered::default();
    for (key, value) in values {
        assert!(recovered.apply_value(key, value).unwrap());
    }
    recovered
}

/// Fire crosses a chunk seam over successive ticks: the west cell burns at
/// tick 1, its ignition is delivered into the east chunk, and the east cell
/// burns at tick 2. Stone containment proves the fire then burns out.
#[test]
fn pin_fire_propagates_across_chunk_seam_over_time() {
    let save = TestDir::new();
    let west = chunk(0, 4, 0);
    let east = chunk(1, 4, 0);
    let mut world = World::with_capacity(61, save.0.clone(), 8).unwrap();
    world.get_chunk(west).unwrap();
    world.get_chunk(east).unwrap();
    // Seam pair (local cells 287 and 272) plus a stone shell so every
    // second-generation ignition targets non-flammable blocks.
    let mut setup = vec![(15, 65, 1, WOOD), (16, 65, 1, WOOD)];
    for (x, y, z) in [
        (14, 65, 1),
        (15, 64, 1),
        (15, 66, 1),
        (15, 65, 0),
        (15, 65, 2),
        (17, 65, 1),
        (16, 64, 1),
        (16, 66, 1),
        (16, 65, 0),
        (16, 65, 2),
    ] {
        setup.push((x, y, z, STONE));
    }
    pin_apply(&mut world, &setup);
    let mut frontier = FireFrontier::default();
    frontier.insert(287, 1).unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(west), &frontier.encode())
        .unwrap();
    let mut fire = FireRuntime::new(recovered, 2).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();

    // Tick 1: the west cell burns and mails an ignition east.
    let wave = fire
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    assert_eq!(wave.transactions[0].owner(), west);
    assert_eq!(wave.transactions[0].burns(), &[287]);
    assert!(
        wave.transactions[0]
            .changes()
            .iter()
            .any(|change| change.key == mailbox_key(east, west)),
        "west burn must mail the east chunk"
    );
    for transaction in wave.transactions {
        pin_commit(&mut fire, &mut world, transaction);
    }
    assert_eq!(world.cached_block(15, 65, 1), Some(AIR));
    assert_eq!(world.cached_block(16, 65, 1), Some(WOOD));

    // Delivery moves the ignition into the east frontier without a world edit.
    let delivery = fire
        .prepare_delivery_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert!(!delivery.transactions.is_empty());
    let east_delivery = delivery
        .transactions
        .iter()
        .find(|transaction| transaction.owner() == east)
        .expect("east chunk must receive its ignition");
    assert!(east_delivery.burns().is_empty());
    assert!(east_delivery.world_edit.is_none());
    for transaction in delivery.transactions {
        pin_commit(&mut fire, &mut world, transaction);
    }
    assert!(fire.pending_destinations().next().is_none());

    // Tick 2: the east cell burns across the seam.
    let (delivered, sources, burned) = pin_tick(&mut fire, &mut world, 2);
    assert_eq!((delivered, sources, burned), (2, 1, 1));
    assert_eq!(world.cached_block(16, 65, 1), Some(AIR));

    // The contained fire burns out: later ticks are completely quiet.
    for tick in 3..=6 {
        let (delivered, sources, burned) = pin_tick(&mut fire, &mut world, tick);
        if tick >= 4 {
            assert_eq!((delivered, sources, burned), (0, 0, 0));
        }
    }
}

/// A glowstone placement ignites its wooden neighbours, the ring burns, and
/// the fire goes out: glowstone and stone survive, wood becomes air, and the
/// frontier disappears.
#[test]
fn pin_glowstone_ignition_burns_wood_ring_then_burns_out() {
    let save = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(62, save.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    // Stone box 0..4 x 64..68 x 0..4 containing one lamp and six wood cells.
    let lamp = (1, 65, 1);
    let ring = [
        (0, 65, 1),
        (2, 65, 1),
        (1, 64, 1),
        (1, 66, 1),
        (1, 65, 0),
        (1, 65, 2),
    ];
    let mut setup = Vec::new();
    for x in 0..4 {
        for y in 64..68 {
            for z in 0..4 {
                if (x, y, z) == lamp {
                    setup.push((x, y, z, GLOWSTONE));
                } else if ring.contains(&(x, y, z)) {
                    setup.push((x, y, z, WOOD));
                } else {
                    setup.push((x, y, z, STONE));
                }
            }
        }
    }
    pin_apply(&mut world, &setup);
    let mut fire = FireRuntime::new(FireRecovered::default(), 2).unwrap();

    // Ignition: placing glowstone mails its six neighbours in one mailbox.
    let seed = fire
        .prepare_seed_from_edit(TickId::new(7), owner, 273, GLOWSTONE)
        .unwrap()
        .expect("glowstone placement must seed fire");
    assert_eq!(seed.changes().len(), 1);
    assert_eq!(seed.changes()[0].key, mailbox_key(owner, owner));
    fire.mark_seed_submitted(&seed).unwrap();
    fire.install_seed_synced(seed).unwrap();

    // Propagation: delivery moves ignitions into the frontier, source burns
    // them. The ring burns exactly once per cell, then silence.
    let mut total_burns = 0;
    let mut quiet_ticks = 0;
    for tick in 8..=24 {
        let (delivered, sources, burned) = pin_tick(&mut fire, &mut world, tick);
        total_burns += burned;
        if delivered == 0 && sources == 0 {
            quiet_ticks += 1;
        } else {
            quiet_ticks = 0;
        }
        if quiet_ticks == 2 {
            break;
        }
    }
    assert_eq!(total_burns, 6, "each ring cell burns exactly once");
    assert_eq!(quiet_ticks, 2, "contained fire must burn out");
    for &(x, y, z) in &ring {
        assert_eq!(world.cached_block(x, y, z), Some(AIR));
    }
    assert_eq!(world.cached_block(lamp.0, lamp.1, lamp.2), Some(GLOWSTONE));
    assert!(
        fire.snapshot()
            .checkpoint_values()
            .iter()
            .all(|(key, _)| *key != frontier_key(owner)),
        "burned-out frontier must disappear"
    );
}

/// Cells whose blocks lack the FLAMMABLE flag are consumed from the frontier
/// without burning: no world edit, stone untouched, wood burned.
#[test]
fn pin_nonflammable_cells_never_burn() {
    let save = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(63, save.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    let mut setup = vec![(1, 65, 1, WOOD), (2, 65, 1, STONE)];
    for (x, y, z) in [
        (0, 65, 1),
        (1, 64, 1),
        (1, 66, 1),
        (1, 65, 0),
        (1, 65, 2),
        (3, 65, 1),
        (2, 64, 1),
        (2, 66, 1),
        (2, 65, 0),
        (2, 65, 2),
    ] {
        setup.push((x, y, z, STONE));
    }
    pin_apply(&mut world, &setup);
    let mut frontier = FireFrontier::default();
    frontier.insert(273, 1).unwrap();
    frontier.insert(274, 1).unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    let mut fire = FireRuntime::new(recovered, 1).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();

    let wave = fire
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    // Both cells are consumed, but only the wooden one burns.
    assert_eq!(wave.transactions[0].burns(), &[273]);
    let edit = wave.transactions[0]
        .world_edit
        .clone()
        .expect("wood burn publishes a world edit");
    assert!(edit.changed);
    for transaction in wave.transactions {
        pin_commit(&mut fire, &mut world, transaction);
    }
    assert_eq!(world.cached_block(1, 65, 1), Some(AIR));
    assert_eq!(world.cached_block(2, 65, 1), Some(STONE));

    // Second-generation ignitions all target stone or air: quiet after one
    // delivery tick, and the frontier is gone.
    let (delivered, _, _) = pin_tick(&mut fire, &mut world, 2);
    assert!(delivered > 0, "emitted ignitions must be consumed");
    let (delivered, sources, burned) = pin_tick(&mut fire, &mut world, 3);
    assert_eq!((delivered, sources, burned), (0, 0, 0));
    assert!(
        fire.snapshot()
            .checkpoint_values()
            .iter()
            .all(|(key, _)| *key != frontier_key(owner))
    );
}

/// Burning a wooden plug open relights the room beyond (glow propagates
/// through the published AIR edit), while a sealed control room stays dark.
#[test]
fn pin_fire_edits_relight_room_and_sealed_cave_stays_dark() {
    let save = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(64, save.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    // Real fire half: a stone-contained wooden plug burns to air.
    pin_apply(
        &mut world,
        &[
            (5, 65, 4, WOOD),
            (4, 65, 4, STONE),
            (6, 65, 4, STONE),
            (5, 64, 4, STONE),
            (5, 66, 4, STONE),
            (5, 65, 3, STONE),
            (5, 65, 5, STONE),
        ],
    );
    let mut frontier = FireFrontier::default();
    frontier.insert(325, 1).unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    let mut fire = FireRuntime::new(recovered, 1).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();
    let wave = fire
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    assert_eq!(wave.transactions[0].burns(), &[325]);
    for transaction in wave.transactions {
        pin_commit(&mut fire, &mut world, transaction);
    }
    // The exact transition the lighting half mirrors below.
    assert_eq!(world.cached_block(5, 65, 4), Some(AIR));

    // Lighting half: a synthetic sealed neighbourhood; lamp at local
    // (4,1,4), plug at (5,1,4), probe room at (6,1,4), sealed control at
    // (10,1,10). Everything else is solid stone.
    let key = chunk(0, 4, 0);
    let mut known: HashMap<ChunkKey, Arc<Chunk>> = HashMap::new();
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let neighbour = ChunkKey {
                    x: key.x + dx,
                    y: key.y + dy,
                    z: key.z + dz,
                };
                known.insert(
                    neighbour,
                    Arc::new(Chunk {
                        key: neighbour,
                        version: 0,
                        blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into(),
                    }),
                );
            }
        }
    }
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    room.blocks.set(Chunk::index([4, 1, 4]).unwrap(), GLOWSTONE);
    room.blocks.set(Chunk::index([5, 1, 4]).unwrap(), WOOD);
    room.blocks.set(Chunk::index([6, 1, 4]).unwrap(), AIR);
    room.blocks.set(Chunk::index([10, 1, 10]).unwrap(), AIR);

    // Plug in place: the probe room is dark, and so is the control.
    let dark = LightField::build(key, &known, 0xB10C_6100);
    let probe = dark.face([5, 1, 4], 0, 1);
    assert_eq!(probe.glow, 0, "wooden plug must block lamp glow");
    assert_eq!(probe.sky, 0);
    let control = dark.face([9, 1, 10], 0, 1);
    assert_eq!(control, LightSample::default());

    // Plug burned away (the transition fire published above): lamp glow
    // reaches the room, the sealed control stays dark.
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    room.blocks.set(Chunk::index([5, 1, 4]).unwrap(), AIR);
    let lit = LightField::build(key, &known, 0xB10C_6100);
    let probe = lit.face([5, 1, 4], 0, 1);
    assert_eq!(probe.sky, 0);
    assert_eq!(probe.glow, 13, "glow falls one level per air cell");
    let control = lit.face([9, 1, 10], 0, 1);
    assert_eq!(control, LightSample::default());
}

/// Restarting from the persisted fire values resumes identical state:
/// checkpoint round-trip is exact and the next wave prepares byte-identical
/// transactions on both runtimes.
#[test]
fn pin_restart_recovers_identical_fire_state() {
    let save = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(65, save.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    pin_apply(
        &mut world,
        &[
            (1, 65, 1, WOOD),
            (0, 65, 1, STONE),
            (1, 64, 1, STONE),
            (1, 66, 1, STONE),
            (1, 65, 0, STONE),
            (1, 65, 2, STONE),
        ],
    );
    let mut frontier = FireFrontier::default();
    frontier.insert(273, 1).unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    let mut fire = FireRuntime::new(recovered, 2).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();

    // Tick 1 burns through the WAL path, then the process "restarts".
    let wave = fire
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    for transaction in wave.transactions {
        pin_commit(&mut fire, &mut world, transaction);
    }
    let values = fire.snapshot().checkpoint_values();
    let restarted = FireRuntime::new(pin_recovered(&values), 2).unwrap();
    assert_eq!(restarted.snapshot().checkpoint_values(), values);

    // Both runtimes prepare the identical next wave from identical state.
    let canonical = |wave: FireWave| {
        wave.transactions
            .into_iter()
            .map(|transaction| (transaction.owner(), transaction.changes().to_vec()))
            .collect::<Vec<_>>()
    };
    let mut fire = fire;
    let mut restarted = restarted;
    let first = canonical(
        fire.prepare_delivery_wave(&mut world, &plan, TickId::new(2))
            .unwrap(),
    );
    let second = canonical(
        restarted
            .prepare_delivery_wave(&mut world, &plan, TickId::new(2))
            .unwrap(),
    );
    assert!(!first.is_empty());
    assert_eq!(first, second);
}

/// A crash between WAL admission and receipt recovers to the last complete
/// record: no partial burn, the frontier is intact, and the retry prepares
/// byte-identical changes.
#[test]
fn pin_crash_mid_commit_recovers_whole_without_partial_burn() {
    let save = TestDir::new();
    let owner = chunk(0, 4, 0);
    let mut world = World::with_capacity(66, save.0.clone(), 8).unwrap();
    world.get_chunk(owner).unwrap();
    pin_apply(
        &mut world,
        &[
            (1, 65, 1, WOOD),
            (0, 65, 1, STONE),
            (2, 65, 1, STONE),
            (1, 64, 1, STONE),
            (1, 66, 1, STONE),
            (1, 65, 0, STONE),
            (1, 65, 2, STONE),
        ],
    );
    let mut frontier = FireFrontier::default();
    frontier.insert(273, 1).unwrap();
    let mut recovered = FireRecovered::default();
    recovered
        .apply_value(&frontier_key(owner), &frontier.encode())
        .unwrap();
    let mut fire = FireRuntime::new(recovered, 1).unwrap();
    let plan = crate::server::builtins::builtin_phase_plan().unwrap();

    // The last complete WAL record, before the doomed wave is prepared.
    let pre_wave = fire.snapshot().checkpoint_values();
    let wave = fire
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(wave.transactions.len(), 1);
    let lost = wave.transactions.into_iter().next().unwrap();
    assert_eq!(lost.burns(), &[273]);
    let lost_changes = lost.changes().to_vec();

    // WAL admission succeeds, then the process crashes before the receipt:
    // no world apply, no receipt install.
    fire.mark_submitted(&lost).unwrap();
    drop(fire);
    assert_eq!(world.cached_block(1, 65, 1), Some(WOOD));
    assert_eq!(world.cached_block(2, 65, 1), Some(STONE));

    // Recovery replays the last complete record; the frontier is intact and
    // the retry prepares the identical atomic change set.
    let mut fire = FireRuntime::new(pin_recovered(&pre_wave), 1).unwrap();
    assert_eq!(fire.snapshot().checkpoint_values(), pre_wave);
    let retry = fire
        .prepare_source_wave(&mut world, &plan, TickId::new(1))
        .unwrap();
    assert_eq!(retry.transactions.len(), 1);
    assert_eq!(retry.transactions[0].burns(), &[273]);
    assert_eq!(retry.transactions[0].changes(), lost_changes.as_slice());
    for transaction in retry.transactions {
        pin_commit(&mut fire, &mut world, transaction);
    }
    // The retried commit lands whole: wood burned, stone untouched.
    assert_eq!(world.cached_block(1, 65, 1), Some(AIR));
    assert_eq!(world.cached_block(2, 65, 1), Some(STONE));
}
