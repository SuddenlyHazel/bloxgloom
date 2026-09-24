use super::*;
use crate::server::durable::CommitAction;
use crate::server::server_state;
use crate::world::{AIR, Chunk, GLOWSTONE, WOOD, world_to_chunk};
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
