use super::*;
use crate::items::{ItemId, STICK};
use crate::server::entities::{CellCoord, KilnSlot, kiln_block_states, kiln_payload};
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
            sent_epochs: Default::default(),
            sent_block_versions: Default::default(),
            sent_entity_revisions: Default::default(),
            next_snapshot_epoch: 1,
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

fn grant_action_epoch(state: &mut State, profile: u128) -> u64 {
    assert!(
        state
            .durability
            .request_epoch_grant(profile, TickId::new(1))
            .unwrap()
            .is_none()
    );
    for _ in 0..2_000 {
        super::super::receipt::poll_journal_receipts(state).unwrap();
        if state.durability.pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    state
        .durability
        .request_epoch_grant(profile, TickId::new(2))
        .unwrap()
        .unwrap()
}

fn settle_live_action(state: &mut State, tick: u64, message: ClientMessage) {
    super::super::coordinator::handle_live_message(state, 1, message, TickId::new(tick)).unwrap();
    for current in tick..tick + 2_000 {
        super::super::coordinator::process_durable_actions(
            state,
            TickId::new(current),
            Instant::now(),
        )
        .unwrap();
        if state.durability.queued.is_empty() && state.durability.pending.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("live durable action did not settle");
}

#[test]
fn admin_grant_requires_server_authorization_and_persists_inventory() {
    let path = temp_save_dir("admin-grant");
    let mut state = server_state(23, path.clone()).unwrap();
    let _peer = add_test_client(&mut state, [0.5, 80.0, 0.5], Inventory::default());
    let epoch = grant_action_epoch(&mut state, 17);
    let item = state.world.catalog().items().next().unwrap().id;
    settle_live_action(
        &mut state,
        3,
        ClientMessage::AdminGive {
            action_id: (u128::from(epoch) << 64) | 1,
            item,
            count: 128,
        },
    );
    assert!(!state.durability.failed);
    assert_eq!(state.clients[&1].inventory.slots[0], None);

    state.admin_profile = Some(17);
    settle_live_action(
        &mut state,
        4,
        ClientMessage::AdminGive {
            action_id: (u128::from(epoch) << 64) | 2,
            item,
            count: 128,
        },
    );
    assert_eq!(
        state.clients[&1].inventory.slots[0],
        Some(crate::inventory::Stack::new(item, 128))
    );
    drop(state);
    let state = server_state(23, path.clone()).unwrap();
    assert_eq!(
        state.inventory_store.load(17).unwrap().slots[0],
        Some(crate::inventory::Stack::new(item, 128))
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
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

    let result = plan_durable_request(&mut state, &request, TickId::new(1));
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

    let result = plan_durable_request(&mut state, &request, TickId::new(1));
    assert!(matches!(result, Err(error) if error.kind() == ErrorKind::WouldBlock));
    assert!(state.loader.is_pending(above));
    assert!(state.world.cached_block(0, target_y, 0).is_some());

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn kiln_plan_spans_vertical_chunk_seam_without_pre_wal_visibility() {
    let path = temp_save_dir("kiln-seam-plan");
    let mut state = server_state(31, path.clone()).unwrap();
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16 - 1;
    let lower = world_to_chunk(0, target_y, 0).0;
    let upper = world_to_chunk(0, target_y + 1, 0).0;
    assert_ne!(lower, upper);
    state.world.get_chunk(lower).unwrap();
    state.world.get_chunk(upper).unwrap();

    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(crate::content::KILN_ITEM, 1));
    let peer = add_test_client(&mut state, [0.5, target_y as f32, -2.5], inventory);
    let facing_east = state
        .world
        .catalog()
        .state_with_property(crate::content::KILN_DEFAULT_STATE, "facing", "east")
        .unwrap();
    let request = edit_request(ClientMessage::Edit {
        action_id: 3,
        x: 0,
        y: target_y,
        z: 0,
        block: facing_east,
        slot: 0,
    });
    let action = plan_durable_request(&mut state, &request, TickId::new(1))
        .unwrap()
        .unwrap();
    assert_eq!(action.world_edits.len(), 2);
    assert_eq!(action.deltas.len(), 2);
    assert_eq!(action.inventory.as_ref().unwrap().slots[0], None);
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();
    assert_eq!(state.entities.len(), 0);
    assert_eq!(state.world.cached_block(0, target_y, 0), Some(AIR));
    assert_eq!(state.world.cached_block(0, target_y + 1, 0), Some(AIR));
    assert_eq!(
        state.clients.get(&1).unwrap().inventory.slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );

    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn kiln_place_interact_tick_restart_and_break_conserve_items_across_seam() {
    let path = temp_save_dir("kiln-live-seam-restart");
    let profile = 17;
    let target_y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16 - 1;
    let anchor = CellCoord::new(-1, target_y, -1);
    let upper = CellCoord::new(-1, target_y + 1, -1);
    let lower_chunk = anchor.chunk();
    let upper_chunk = upper.chunk();
    assert_ne!(lower_chunk, upper_chunk);

    let mut state = server_state(53, path.clone()).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(crate::content::KILN_ITEM, 1));
    inventory.slots[1] = Some(crate::inventory::Stack::new(STICK, 1));
    inventory.slots[2] = Some(crate::inventory::Stack::new(
        ItemId(crate::world::GRAVEL.0),
        1,
    ));
    let player_peer = add_test_client(
        &mut state,
        [
            anchor.x as f32 + 0.5,
            anchor.y as f32,
            anchor.z as f32 + 3.5,
        ],
        inventory,
    );
    state.world.get_chunk(lower_chunk).unwrap();
    state.world.get_chunk(upper_chunk).unwrap();
    assert_eq!(
        state.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(AIR)
    );
    assert_eq!(
        state.world.cached_block(upper.x, upper.y, upper.z),
        Some(AIR)
    );

    let epoch = grant_action_epoch(&mut state, profile);
    let action_id = |sequence: u64| (u128::from(epoch) << 64) | u128::from(sequence);
    settle_live_action(
        &mut state,
        10,
        ClientMessage::Edit {
            action_id: action_id(1),
            x: anchor.x,
            y: anchor.y,
            z: anchor.z,
            block: crate::content::KILN_DEFAULT_STATE,
            slot: 0,
        },
    );
    let entity_id = state
        .entities
        .anchored_at(anchor)
        .expect("WAL-installed kiln");
    assert_eq!(state.entities.anchored_at(upper), Some(entity_id));
    assert_eq!(
        state.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(crate::content::KILN_DEFAULT_STATE)
    );

    let fuel_request = ClientMessage::EntityInteract {
        action_id: action_id(2),
        target: [anchor.x, anchor.y, anchor.z],
        payload: vec![1, 0, 0, 1, 1, 0], // insert one stick as fuel
    };
    super::super::coordinator::handle_live_message(&mut state, 1, fuel_request, TickId::new(11))
        .unwrap();
    super::super::coordinator::process_durable_actions(&mut state, TickId::new(11), Instant::now())
        .unwrap();
    let lower_key = super::super::chunk_state_key(lower_chunk);
    let upper_key = super::super::chunk_state_key(upper_chunk);
    assert!(state.durability.reserved.contains(&lower_key));
    assert!(state.durability.reserved.contains(&upper_key));
    let staged = match &state.durability.pending[0].payload {
        super::super::PendingPayload::Action(action) => action,
        super::super::PendingPayload::Fire(_) => panic!("kiln action staged as fire"),
    };
    assert!(staged.world_edits.is_empty());
    assert!(
        staged
            .entities
            .as_ref()
            .unwrap()
            .changes()
            .iter()
            .all(|change| change.key.domain != "bloxgloom:chunk_snapshot")
    );

    let overlapping_edit = state
        .world
        .prepare_edits(&[(upper.x, upper.y, upper.z, crate::world::STONE)])
        .unwrap();
    let overlapping_action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: overlapping_edit,
        drops: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: None,
    };
    assert!(matches!(
        state
            .durability
            .try_stage(TickId::new(12), &overlapping_action, None, None),
        Err(super::super::StageError::Conflict)
    ));
    for current in 12..2_012 {
        super::super::coordinator::process_durable_actions(
            &mut state,
            TickId::new(current),
            Instant::now(),
        )
        .unwrap();
        if state.durability.pending.is_empty() && state.durability.queued.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!state.durability.reserved.contains(&lower_key));
    assert!(!state.durability.reserved.contains(&upper_key));
    settle_live_action(
        &mut state,
        12,
        ClientMessage::EntityInteract {
            action_id: action_id(3),
            target: [anchor.x, anchor.y, anchor.z],
            payload: vec![1, 0, 1, 2, 1, 0], // insert one gravel input
        },
    );
    assert_eq!(state.clients[&1].inventory.slots[1], None);
    assert_eq!(state.clients[&1].inventory.slots[2], None);

    for tick in [30, 50, 70, 90] {
        super::super::coordinator::queue_interaction_actions(&mut state, TickId::new(tick));
        for current in tick..tick + 2_000 {
            super::super::coordinator::process_durable_actions(
                &mut state,
                TickId::new(current),
                Instant::now(),
            )
            .unwrap();
            if state.durability.queued.is_empty() && state.durability.pending.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(state.durability.pending.is_empty());
        assert!(state.durability.queued.is_empty());
    }
    let cooked = state.entities.snapshot(entity_id).unwrap();
    let payload = kiln_payload(&cooked).unwrap();
    assert_eq!(
        payload.slot(KilnSlot::Output),
        Some(&crate::inventory::Stack::new(
            ItemId(crate::world::STONE.0),
            1
        ))
    );
    assert!(payload.is_lit());
    let lit_states = kiln_block_states(state.world.catalog(), payload).unwrap();
    assert_eq!(
        state.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(lit_states[0])
    );
    assert_eq!(
        state.world.cached_block(upper.x, upper.y, upper.z),
        Some(lit_states[1])
    );
    assert_eq!(state.durability.receipt_ledger(profile).results.len(), 3);
    drop(player_peer);
    drop(state);

    let mut recovered = server_state(53, path.clone()).unwrap();
    recovered.world.get_chunk(lower_chunk).unwrap();
    recovered.world.get_chunk(upper_chunk).unwrap();
    let recovered_id = recovered
        .entities
        .anchored_at(anchor)
        .expect("recovered kiln");
    assert_eq!(recovered_id, entity_id);
    let recovered_snapshot = recovered.entities.snapshot(recovered_id).unwrap();
    let recovered_payload = kiln_payload(&recovered_snapshot).unwrap();
    assert!(recovered_payload.is_lit());
    assert_eq!(
        recovered_payload.slot(KilnSlot::Output),
        Some(&crate::inventory::Stack::new(
            ItemId(crate::world::STONE.0),
            1
        ))
    );
    let recovered_states = kiln_block_states(recovered.world.catalog(), recovered_payload).unwrap();
    assert_eq!(
        recovered.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(recovered_states[0])
    );
    assert_eq!(
        recovered.world.cached_block(upper.x, upper.y, upper.z),
        Some(recovered_states[1])
    );

    let inventory = recovered.inventory_store.load(profile).unwrap();
    let peer = add_test_client(
        &mut recovered,
        [
            anchor.x as f32 + 0.5,
            anchor.y as f32,
            anchor.z as f32 + 3.5,
        ],
        inventory,
    );
    settle_live_action(
        &mut recovered,
        100,
        ClientMessage::Edit {
            action_id: action_id(4),
            x: upper.x,
            y: upper.y,
            z: upper.z,
            block: AIR,
            slot: 0,
        },
    );
    assert_eq!(recovered.entities.anchored_at(anchor), None);
    assert_eq!(recovered.entities.anchored_at(upper), None);
    assert_eq!(
        recovered.world.cached_block(anchor.x, anchor.y, anchor.z),
        Some(AIR)
    );
    assert_eq!(
        recovered.world.cached_block(upper.x, upper.y, upper.z),
        Some(AIR)
    );
    let drop_position = [
        anchor.x as f32 + 0.5,
        anchor.y as f32 + 0.5,
        anchor.z as f32 + 0.5,
    ];
    let stacks: Vec<_> = recovered
        .drops
        .nearby(drop_position)
        .iter()
        .map(|drop| recovered.drops.stack(drop.id).unwrap())
        .collect();
    assert_eq!(stacks.len(), 2);
    assert_eq!(stacks.iter().map(|stack| stack.count).sum::<u16>(), 2);
    assert!(
        stacks
            .iter()
            .any(|stack| stack.item == crate::content::KILN_ITEM)
    );
    assert!(
        stacks
            .iter()
            .any(|stack| stack.item == ItemId(crate::world::STONE.0))
    );

    drop(peer);
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}
