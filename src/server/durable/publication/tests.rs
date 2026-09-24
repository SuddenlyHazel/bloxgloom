use super::*;
use crate::server::durable::BlockDelta;
use crate::server::server_state;
use crate::world::{AIR, GRASS, STONE, world_to_chunk};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_save_dir() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "bloxgloom-durable-multiedit-{}-{stamp}",
        std::process::id()
    ))
}

#[test]
fn several_cells_in_one_chunk_publish_one_atomic_commit_part() {
    let path = temp_save_dir();
    let mut state = server_state(53, path.clone()).unwrap();
    let coords = [(10, 80, 10, GRASS), (11, 80, 10, STONE)];
    let prepared = state.world.prepare_edits(&coords).unwrap();
    let (key, first_local) = world_to_chunk(10, 80, 10);
    let (_, second_local) = world_to_chunk(11, 80, 10);
    let version = prepared
        .iter()
        .find(|edit| edit.key == key)
        .unwrap()
        .new_version;
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: prepared,
        drops: Default::default(),
        deltas: vec![
            BlockDelta {
                key,
                version,
                local: first_local.map(|part| part as u8),
                block: GRASS,
            },
            BlockDelta {
                key,
                version,
                local: second_local.map(|part| part as u8),
                block: STONE,
            },
        ],
        changed_cells: vec![CellCoord::new(10, 80, 10), CellCoord::new(11, 80, 10)],
        pickups: Vec::new(),
        fire_seed: None,
        entities: None,
    };

    apply_committed_action(&mut state, action, None).unwrap();

    let published = state.durability.publish_queue.pop().unwrap();
    assert_eq!(published.deltas.len(), 2);
    assert!(
        published
            .deltas
            .iter()
            .all(|delta| delta.version == version)
    );
    let chunk = state.world.cached_chunk(key).unwrap();
    assert_eq!(chunk.version, version);
    assert_eq!(chunk.block(first_local), Some(GRASS));
    assert_eq!(chunk.block(second_local), Some(STONE));
    assert_eq!(state.world.cached_block(12, 80, 10), Some(AIR));
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
