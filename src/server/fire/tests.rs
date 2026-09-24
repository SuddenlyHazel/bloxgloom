use super::codec::{key_bytes, read_key};
use super::scheduler::{cursor_key, frontier_key, mailbox_key, owner_lane};
use super::*;
use crate::server::journal::StateKey;
use crate::server::simulation::TickId;
use crate::world::{CHUNK_VOLUME, ChunkKey, GLOWSTONE, STONE, WOOD, World, world_to_chunk};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
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

    let path = directory.0.join("fire/cursor_00.fire");
    let mut corrupt = fs::read(&path).unwrap();
    corrupt[8] ^= 0x80;
    fs::write(&path, corrupt).unwrap();
    assert!(store.read(&key).is_err());
    store.write(&key, &value).unwrap();
    store.write(&key, &[]).unwrap();
    assert!(store.read(&key).unwrap().is_none());
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
