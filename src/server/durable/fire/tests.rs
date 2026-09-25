use super::*;
use crate::server::durable::CommitAction;
use crate::server::journal::StateKey;
use crate::server::server_state;
use crate::world::{AIR, Chunk, GLOWSTONE, WOOD, world_to_chunk};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temp_save() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bloxgloom-live-fire-{}-{nonce}",
        std::process::id()
    ))
}

fn drain_wal(state: &mut State) {
    for _ in 0..1_000 {
        super::super::receipt::poll_journal_receipts(state).unwrap();
        if state.durability.pending.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("fire WAL did not produce a receipt");
}

#[test]
fn seeded_fire_survives_restart_and_burns_only_after_wal_receipt() {
    let path = temp_save();
    let mut state = server_state(71, path.clone()).unwrap();
    let (source, local) = world_to_chunk(8, 96, 8);
    let cell = Chunk::index(local).unwrap() as u16;
    let edits = state
        .world
        .prepare_edits(&[(8, 96, 8, GLOWSTONE), (9, 96, 8, WOOD)])
        .unwrap();
    let seed = state
        .fire
        .prepare_seed_from_edit(TickId::new(1), source, cell, GLOWSTONE)
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: Some(seed.clone()),
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None, None)
            .unwrap()
    );
    state.fire.mark_seed_submitted(&seed).unwrap();
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 96, 8), Some(WOOD));
    drop(state);

    let mut state = server_state(71, path.clone()).unwrap();
    assert_eq!(state.world.cached_block(9, 96, 8), Some(WOOD));
    assert!(
        state
            .fire
            .snapshot()
            .checkpoint_values()
            .iter()
            .any(|(key, value)| key.domain == "bloxgloom:fire_pending" && !value.is_empty())
    );
    run_delivery(&mut state, TickId::new(3)).unwrap();
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 96, 8), Some(WOOD));
    run_source(&mut state, TickId::new(4)).unwrap();
    drain_wal(&mut state);
    assert_eq!(state.world.cached_block(9, 96, 8), Some(AIR));
    drop(state);

    // The synced BGED/fire record, not the in-memory owner slot, must be
    // sufficient to restore the burn after a second process restart.
    let state = server_state(71, path.clone()).unwrap();
    assert_eq!(state.world.cached_block(9, 96, 8), Some(AIR));
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn dirty_fire_keys_use_one_checkpoint_job_and_revision_fenced_receipt() {
    let path = temp_save();
    let mut state = server_state(73, path.clone()).unwrap();
    let (source, local) = world_to_chunk(8, 96, 8);
    let cell = Chunk::index(local).unwrap() as u16;
    let edits = state.world.prepare_edits(&[(8, 96, 8, GLOWSTONE)]).unwrap();
    let seed = state
        .fire
        .prepare_seed_from_edit(TickId::new(1), source, cell, GLOWSTONE)
        .unwrap()
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: edits,
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: Some(seed.clone()),
        entities: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None, None)
            .unwrap()
    );
    state.fire.mark_seed_submitted(&seed).unwrap();
    drain_wal(&mut state);

    let fire_count = state
        .durability
        .dirty_checkpoints
        .keys()
        .filter(|key| {
            matches!(
                key.domain.as_str(),
                "bloxgloom:fire_frontier" | "bloxgloom:fire_pending" | "bloxgloom:fire_cursor"
            )
        })
        .count();
    assert!(fire_count > 1);
    super::super::checkpoint::submit_dirty_checkpoints(&mut state);
    let batch = state.durability.fire_checkpoint_batch.as_ref().unwrap();
    assert_eq!(batch.covered.len(), fire_count);
    let revised_key = batch.covered[0].0.clone();
    let revised_snapshot = state.durability.dirty_checkpoints[&revised_key]
        .snapshot
        .clone();
    state
        .durability
        .remember_checkpoint(revised_key.clone(), revised_snapshot);
    assert!(
        state
            .durability
            .checkpoint_inflight
            .contains_key(&StateKey::new(
                "bloxgloom:fire_checkpoint_batch",
                Vec::new()
            ))
    );

    for _ in 0..1_000 {
        super::super::checkpoint::process_checkpoint_receipts(
            &mut state,
            std::time::Instant::now(),
        );
        if state.durability.fire_checkpoint_batch.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(state.durability.fire_checkpoint_batch.is_none());
    assert!(
        state
            .durability
            .dirty_checkpoints
            .contains_key(&revised_key)
    );
    assert_eq!(
        state
            .durability
            .dirty_checkpoints
            .keys()
            .filter(|key| matches!(
                key.domain.as_str(),
                "bloxgloom:fire_frontier" | "bloxgloom:fire_pending" | "bloxgloom:fire_cursor"
            ))
            .count(),
        1
    );
    super::super::checkpoint::submit_dirty_checkpoints(&mut state);
    assert_eq!(
        state
            .durability
            .fire_checkpoint_batch
            .as_ref()
            .unwrap()
            .covered,
        vec![(
            revised_key.clone(),
            state.durability.dirty_checkpoints[&revised_key].revision
        )]
    );
    for _ in 0..1_000 {
        super::super::checkpoint::process_checkpoint_receipts(
            &mut state,
            std::time::Instant::now(),
        );
        if state.durability.fire_checkpoint_batch.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(state.durability.fire_checkpoint_batch.is_none());
    assert!(
        !state
            .durability
            .dirty_checkpoints
            .contains_key(&revised_key)
    );
    assert!(path.join("fire/checkpoints.fire").exists());
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn checkpoint_pressure_admits_a_durable_owner_prefix_without_losing_the_remainder() {
    let path = temp_save();
    let mut state = server_state(72, path.clone()).unwrap();
    assert!(state.durability.dirty_checkpoints.is_empty());
    for index in 0..MAX_DIRTY_CHECKPOINT_KEYS - 2 {
        state.durability.remember_checkpoint(
            StateKey::new("test:pressure", (index as u32).to_le_bytes().to_vec()),
            vec![1],
        );
    }
    let owners: BTreeMap<_, _> = (0..32)
        .map(|x| (ChunkKey { x, y: 4, z: 0 }, vec![256]))
        .collect();
    let wave = state
        .fire
        .prepare_benchmark_frontier_wave(&owners, TickId::new(1))
        .unwrap();
    let candidates = wave.transactions.len();
    assert!(candidates > 1);
    stage_wave(&mut state, TickId::new(1), wave).unwrap();
    assert_eq!(state.durability.pending.len(), 1);
    assert_eq!(state.fire.load_metrics().admitted_transactions, 1);
    assert_eq!(
        state.fire.load_metrics().full_deferred_transactions,
        (candidates - 1) as u64
    );
    drain_wal(&mut state);
    assert_eq!(state.fire.load_metrics().frontier_cells, 1);
    drop(state);

    let state = server_state(72, path.clone()).unwrap();
    assert_eq!(state.fire.load_metrics().frontier_cells, 1);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
