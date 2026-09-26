use super::*;
use crate::server::server_state;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn bounded_dispatch_passes_blocked_keys_and_retries_newer_revisions() {
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-checkpoint-turn-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut state = server_state(73, path.clone()).unwrap();
    let bytes = InventoryStore::encode_snapshot(&Inventory::default()).unwrap();
    let mut keys = Vec::new();
    for profile in 1..=40 {
        let key = inventory_state_key(profile);
        state
            .durability
            .remember_checkpoint(key.clone(), bytes.clone());
        keys.push(key);
    }
    keys.sort();
    for key in keys.iter().take(32) {
        state
            .durability
            .dirty_checkpoints
            .get_mut(key)
            .unwrap()
            .retry_after = Instant::now() + Duration::from_secs(3600);
    }
    submit_dirty_checkpoints(&mut state);
    assert_eq!(state.durability.checkpoint_cursor.as_ref(), Some(&keys[15]));
    assert!(state.durability.checkpoint_inflight.is_empty());
    submit_dirty_checkpoints(&mut state);
    assert_eq!(state.durability.checkpoint_cursor.as_ref(), Some(&keys[31]));
    assert!(state.durability.checkpoint_inflight.is_empty());
    submit_dirty_checkpoints(&mut state);
    assert_eq!(state.durability.checkpoint_cursor.as_ref(), Some(&keys[7]));
    assert_eq!(state.durability.checkpoint_inflight.len(), 8);

    // A new revision while the old write is queued/completed must remain dirty.
    let revised = keys[39].clone();
    state.durability.remember_checkpoint(revised.clone(), bytes);
    state.durability.checkpoint_writer.shutdown().unwrap(); // explicit completion
    process_checkpoint_receipts(&mut state, Instant::now());
    assert!(state.durability.dirty_checkpoints.contains_key(&revised));
    assert_eq!(state.durability.dirty_checkpoints.len(), 33);
    for key in &keys[32..39] {
        assert!(!state.durability.dirty_checkpoints.contains_key(key));
    }
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
