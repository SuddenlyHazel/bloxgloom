use super::*;
use crate::server::movement::MovementState;
use crate::server::{Client, DEFAULT_VIEW, State, server_state};
use crate::world::{GRASS, MAX_GENERATED_HEIGHT, RED_FLOWER};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_save_dir(label: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("bloxgloom-{label}-{}-{stamp}", std::process::id()))
}

fn add_test_client(state: &mut State, position: [f32; 3], inventory: Inventory) -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, _receiver) = state.outbound.client_queue();
    let center = world_to_chunk(
        position[0].floor() as i32,
        position[1].floor() as i32,
        position[2].floor() as i32,
    )
    .0;
    state.clients.insert(
        1,
        Client {
            profile: 17,
            inventory,
            last_drops_revision: u64::MAX,
            last_drop_anchor: [i32::MAX; 3],
            last_sent_drops: Vec::new(),
            sender,
            socket,
            sent: Default::default(),
            center,
            radius: DEFAULT_VIEW,
            movement: MovementState::new(position, 0),
            pending_moves: Default::default(),
        },
    );
    peer
}

fn edit_request(message: ClientMessage) -> DurableRequest {
    DurableRequest::Command {
        id: 1,
        message,
        queued_at: Instant::now(),
    }
}

#[test]
fn missing_plant_support_requests_its_exact_vertical_neighbor() {
    let path = temp_save_dir("edit-support-prefetch");
    let mut state = server_state(23, path.clone()).unwrap();
    state.world.reset_cache_for_test(1);
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16;
    let target = world_to_chunk(0, target_y, 0).0;
    let support = world_to_chunk(0, target_y - 1, 0).0;
    assert_ne!(target, support);
    state.world.get_chunk(target).unwrap();

    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(
        crate::items::ItemId::new(RED_FLOWER.get()),
        1,
    ));
    let peer = add_test_client(&mut state, [0.5, target_y as f32, 0.5], inventory);
    let request = edit_request(ClientMessage::Edit {
        action_id: 1,
        x: 0,
        y: target_y,
        z: 0,
        block: RED_FLOWER,
        slot: 0,
    });

    let result = plan_durable_request(&mut state, &request);
    let error = result.err().expect("missing support must defer the edit");
    assert_eq!(error.kind(), ErrorKind::WouldBlock, "{error}");
    assert!(state.loader.is_pending(support));
    assert!(state.world.cached_block(0, target_y, 0).is_some());

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn missing_plant_check_requests_the_exact_above_chunk() {
    let path = temp_save_dir("edit-above-prefetch");
    let mut state = server_state(29, path.clone()).unwrap();
    // A one-chunk cache lets the test keep the harvest target resident while
    // its vertical neighbor remains absent.
    state.world.reset_cache_for_test(1);
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16 - 1;
    let target = world_to_chunk(0, target_y, 0).0;
    let above = world_to_chunk(0, target_y + 1, 0).0;
    assert_ne!(target, above);
    state.world.get_chunk(target).unwrap();
    state.world.edit(0, target_y, 0, GRASS).unwrap();

    let peer = add_test_client(
        &mut state,
        [0.5, target_y as f32, 0.5],
        Inventory::default(),
    );
    let request = edit_request(ClientMessage::Edit {
        action_id: 2,
        x: 0,
        y: target_y,
        z: 0,
        block: crate::world::AIR,
        slot: 0,
    });

    let result = plan_durable_request(&mut state, &request);
    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::WouldBlock));
    assert!(state.loader.is_pending(above));
    assert!(state.world.cached_block(0, target_y, 0).is_some());

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
