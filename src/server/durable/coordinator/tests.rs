//! WAL, checkpoint, and rotation tests for the compact receipt ledger.

use super::*;
use crate::server::server_state;
use crate::server::simulation::TickId;
use crate::world::STONE;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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

fn id(epoch: u64, seq: u64) -> u128 {
    (u128::from(epoch) << 64) | u128::from(seq)
}

fn poll_until_settled(state: &mut State) {
    for _ in 0..2_000 {
        super::super::receipt::poll_journal_receipts(state).unwrap();
        if state.durability.pending.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("WAL receipt did not arrive");
}

fn grant(state: &mut State, profile: u128) -> u64 {
    assert!(
        state
            .durability
            .request_epoch_grant(profile, TickId::new(1))
            .unwrap()
            .is_none()
    );
    poll_until_settled(state);
    let epoch = state
        .durability
        .request_epoch_grant(profile, TickId::new(2))
        .unwrap()
        .unwrap();
    assert_eq!(state.durability.claim_epoch_grant(profile), Some(epoch));
    epoch
}

fn inventory_action(state: &State, profile: u128, action_id: u128) -> CommitAction {
    let mut before = Inventory::default();
    before.slots[0] = Some(crate::inventory::Stack::new(
        crate::items::ItemId::new(STONE.get()),
        2,
    ));
    let mut after = before.clone();
    assert!(after.transfer(0, 1, 1));
    let message = ClientMessage::InventoryMove {
        action_id,
        from: 0,
        to: 1,
        count: 1,
    };
    let payload = durable::encode_action_receipt(&message).unwrap();
    let before_ledger = state.durability.receipt_ledger(profile);
    let result = ResultRecord {
        payload: payload.clone(),
        accepted: true,
        reason: String::new(),
    };
    let after_ledger = before_ledger.append_result(result.clone()).unwrap();
    CommitAction {
        client_id: None,
        profile: Some(profile),
        action_id: Some(action_id),
        receipt_value: Some(payload),
        receipt_transition: Some(
            ReceiptTransition::new(
                profile,
                &before_ledger,
                after_ledger,
                ReceiptEvent::Result(result),
            )
            .unwrap(),
        ),
        inventory_before: Some(InventoryStore::encode_snapshot(&before).unwrap()),
        terrain_reads: Default::default(),
        inventory: Some(after),
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entity_wakes: Vec::new(),
        entities: None,
    }
}

#[test]
fn wal_replay_keeps_result_and_world_effect_before_checkpoint() {
    let path = temp_save_dir("receipt-replay");
    let mut state = server_state(43, path.clone()).unwrap();
    let epoch = grant(&mut state, 91);
    let action_id = id(epoch, 1);
    let action = inventory_action(&state, 91, action_id);
    state
        .inventory_store
        .checkpoint_snapshot(91, action.inventory_before.as_ref().unwrap())
        .unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(3), &action, None)
            .unwrap()
    );
    poll_until_settled(&mut state);
    assert_eq!(state.durability.receipt_ledger(91).results.len(), 1);
    drop(state);

    let recovered = server_state(43, path.clone()).unwrap();
    let ledger = recovered.durability.receipt_ledger(91);
    assert!(matches!(
        ledger.admission(action_id, action.receipt_value.as_ref().unwrap()),
        Admission::Replay(_)
    ));
    let inventory = recovered.inventory_store.load(91).unwrap();
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 1);
    assert_eq!(inventory.slots[1].as_ref().unwrap().count, 1);
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn acknowledged_result_stays_retired_across_rotation_and_restart() {
    let path = temp_save_dir("receipt-rotation");
    let mut state = server_state(37, path.clone()).unwrap();
    let epoch = grant(&mut state, 41);
    let action_id = id(epoch, 1);
    let action = inventory_action(&state, 41, action_id);
    state
        .inventory_store
        .checkpoint_snapshot(41, action.inventory_before.as_ref().unwrap())
        .unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(3), &action, None)
            .unwrap()
    );
    poll_until_settled(&mut state);
    assert!(
        state
            .durability
            .stage_action_ack(41, epoch, 1, TickId::new(4))
            .unwrap()
    );
    poll_until_settled(&mut state);
    state.durability.rotation_requested = true;
    for tick in 5..2_000 {
        process_durable_actions(&mut state, TickId::new(tick), Instant::now()).unwrap();
        if !state.durability.rotation_requested {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!state.durability.rotation_requested);
    drop(state);

    let recovered = server_state(37, path.clone()).unwrap();
    let ledger = recovered.durability.receipt_ledger(41);
    assert_eq!(ledger.acknowledged, 1);
    assert!(ledger.results.is_empty());
    assert!(matches!(
        ledger.admission(action_id, action.receipt_value.as_ref().unwrap()),
        Admission::Retired
    ));
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}
