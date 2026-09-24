//! Live durability coordinator regression tests.

use super::*;
use crate::server::server_state;
use crate::server::simulation::TickId;
use crate::world::STONE;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn action_receipt_capacity_reserves_in_flight_wal_commits() {
    const LIMIT: usize = 4;
    assert!(!super::super::action_receipt_limit_reached(3, 0, LIMIT));
    assert!(super::super::action_receipt_limit_reached(3, 1, LIMIT));
    assert!(super::super::action_receipt_limit_reached(4, 0, LIMIT));
    assert!(super::super::action_receipt_limit_reached(
        usize::MAX,
        1,
        LIMIT
    ));
}

fn temp_save_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bloxgloom-durable-{label}-{}-{stamp}",
        std::process::id()
    ))
}

fn inventory_action(profile: u128, action_id: u128) -> CommitAction {
    let mut before = Inventory::default();
    before.slots[0] = Some(crate::inventory::Stack {
        item: STONE,
        count: 2,
    });
    let mut after = before.clone();
    assert!(after.transfer(0, 1, 1));
    let message = ClientMessage::InventoryMove {
        action_id,
        from: 0,
        to: 1,
        count: 1,
    };
    CommitAction {
        client_id: None,
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(durable::encode_action_receipt(&message).unwrap()),
        inventory_before: Some(InventoryStore::encode_snapshot(&before).unwrap()),
        inventory: Some(after),
        world_edits: Vec::new(),
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
    }
}

#[test]
fn rotation_drains_accepted_actions_checkpoints_them_then_preserves_replay_state() {
    let path = temp_save_dir("rotation-drain");
    let mut state = server_state(37, path.clone()).unwrap();
    let first = inventory_action(41, 101);
    let second = inventory_action(42, 102);
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &first, None)
            .unwrap()
    );
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &second, None)
            .unwrap()
    );
    state.durability.rotation_requested = true;

    assert!(super::super::rotation::progress_rotation(&mut state).unwrap());
    assert_eq!(state.durability.pending.len(), 2);
    assert!(!state.durability.rotation_snapshot_ready);
    assert!(state.durability.rotation_receipt.is_none());

    for tick in 2..2_000 {
        process_durable_actions(&mut state, TickId::new(tick), Instant::now()).unwrap();
        if !state.durability.rotation_requested {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !state.durability.rotation_requested,
        "rotation did not complete"
    );
    assert!(state.durability.pending.is_empty());
    assert!(state.durability.dirty_checkpoints.is_empty());
    assert!(state.durability.checkpoint_inflight.is_empty());
    assert_eq!(state.durability.writer.sequence(), 2);
    drop(state);

    let recovered = server_state(37, path.clone()).unwrap();
    for (profile, action_id) in [(41, 101), (42, 102)] {
        let actual = recovered
            .durability
            .action_receipt(profile, action_id)
            .expect("receipt survives generation rotation");
        let expected = durable::encode_action_receipt(&ClientMessage::InventoryMove {
            action_id,
            from: 0,
            to: 1,
            count: 1,
        })
        .unwrap();
        assert_eq!(actual, expected);
        let inventory = recovered.inventory_store.load(profile).unwrap();
        assert_eq!(inventory.slots[0].unwrap().count, 1);
        assert_eq!(inventory.slots[1].unwrap().count, 1);
    }
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn action_receipt_and_inventory_replay_after_wal_sync_without_checkpoint() {
    let path = temp_save_dir("receipt-replay");
    let action = inventory_action(91, 9001);
    InventoryStore::new(&path)
        .unwrap()
        .checkpoint_snapshot(91, action.inventory_before.as_ref().unwrap())
        .unwrap();
    let mut state = server_state(43, path.clone()).unwrap();
    let receipt_value = action.receipt_value.clone().unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );

    for _ in 0..1_000 {
        super::super::receipt::poll_journal_receipts(&mut state).unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(state.durability.pending.is_empty());
    assert_eq!(
        state
            .durability
            .dirty_checkpoints
            .get(&durable::inventory_state_key(91))
            .unwrap()
            .snapshot,
        InventoryStore::encode_snapshot(action.inventory.as_ref().unwrap()).unwrap()
    );
    assert!(
        !state
            .durability
            .checkpoint_inflight
            .contains_key(&durable::inventory_state_key(91))
    );
    drop(state);

    let recovered = server_state(43, path.clone()).unwrap();
    assert_eq!(
        recovered.durability.action_receipt(91, 9001),
        Some(receipt_value.as_slice())
    );
    let inventory = recovered.inventory_store.load(91).unwrap();
    assert_eq!(inventory.slots[0].unwrap().count, 1);
    assert_eq!(inventory.slots[1].unwrap().count, 1);
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}
