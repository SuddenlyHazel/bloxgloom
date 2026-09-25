use super::*;

const STONE_ITEM: crate::items::ItemId = crate::items::ItemId::new(crate::world::STONE.get());
use std::fs;
use std::path::PathBuf;

fn action_id(state: &State, profile: u128, seq: u64) -> u128 {
    (u128::from(state.durability.receipt_ledger(profile).current_epoch()) << 64) | u128::from(seq)
}

/// Captures every file in the sharded drop base so a test can restore the
/// exact older checkpoint bytes and replay journal recovery over them.
fn snapshot_drop_shards(save: &TestSave) -> Vec<(PathBuf, Vec<u8>)> {
    let dir = save.path().join("drops.d");
    match fs::read_dir(&dir) {
        Ok(listing) => listing
            .flatten()
            .map(|entry| {
                let path = entry.path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn restore_drop_shards(save: &TestSave, snapshot: &[(PathBuf, Vec<u8>)]) {
    let dir = save.path().join("drops.d");
    if let Ok(listing) = fs::read_dir(&dir) {
        for entry in listing.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
    for (path, bytes) in snapshot {
        fs::write(path, bytes).unwrap();
    }
}

#[test]
fn rejected_action_is_durable_then_acknowledged_without_reopening_its_sequence() {
    let save = TestSave::new("rejected-receipt-ack");
    let profile = 991;
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let rejected_id = action_id(&state, profile, 1);
    let rejected = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        1,
        rejected_id,
        ClientMessage::InventoryMove {
            action_id: rejected_id,
            from: 0,
            to: 1,
            count: 1,
        },
    );
    assert_eq!(action_result(&rejected, rejected_id), Some(false));
    assert_eq!(
        state.durability.receipt_ledger(profile).outstanding_len(),
        1
    );
    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: session.id,
            sequence: 2,
            message: ClientMessage::ActionAck {
                epoch: (rejected_id >> 64) as u64,
                through_seq: 1,
            },
        }],
    );
    for _ in 0..500 {
        if state.durability.receipt_ledger(profile).acknowledged_seq() == 1 {
            break;
        }
        run_empty_tick(&mut state, &mut tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        state.durability.receipt_ledger(profile).acknowledged_seq(),
        1
    );
    assert_eq!(
        state.durability.receipt_ledger(profile).outstanding_len(),
        0
    );
    drop(session);
    drop(state);

    let mut restarted = state_for(&save, 7);
    let mut restarted_tick = 1;
    let rejoined = join(&mut restarted, &mut restarted_tick, profile);
    assert_ne!(action_id(&restarted, profile, 1), rejected_id);
    let stale = command_and_wait(
        &mut restarted,
        &mut restarted_tick,
        &rejoined,
        1,
        rejected_id,
        ClientMessage::InventoryMove {
            action_id: rejected_id,
            from: 0,
            to: 1,
            count: 1,
        },
    );
    assert_eq!(action_result(&stale, rejected_id), Some(false));
    assert_eq!(
        restarted
            .durability
            .receipt_ledger(profile)
            .outstanding_len(),
        0
    );
}

#[test]
fn deferred_join_refreshes_inventory_captured_before_a_checkpoint() {
    let save = TestSave::new("stale-join-inventory");
    let profile = 611;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 2));
    save_inventory(&save, profile, &inventory);
    let mut state = state_for(&save, 7);
    let stale = state.inventory_store.load(profile).unwrap();
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let move_id = action_id(&state, profile, 1);

    let result = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        1,
        move_id,
        ClientMessage::InventoryMove {
            action_id: move_id,
            from: 0,
            to: 1,
            count: 1,
        },
    );
    assert_eq!(action_result(&result, move_id), Some(true));
    state.clients.remove(&session.id);

    for _ in 0..500 {
        if !state.durability.inventory_overlay.contains_key(&profile) {
            break;
        }
        run_empty_tick(&mut state, &mut tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !state.durability.inventory_overlay.contains_key(&profile),
        "inventory checkpoint did not complete"
    );
    let fresh = state.inventory_store.load(profile).unwrap();
    assert_ne!(fresh.revision, stale.revision);
    assert_eq!(
        state.durability.inventory_revisions.get(&profile),
        Some(&fresh.revision)
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let _peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, _receiver) = state.outbound.client_queue();
    let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Join {
            profile,
            inventory: Box::new(stale),
            sender,
            socket,
            reply: reply_sender,
        }],
    );
    assert!(matches!(
        reply_receiver.try_recv().unwrap(),
        JoinResponse::RefreshInventory
    ));
    assert!(state.clients.is_empty());

    let rejoined = join(&mut state, &mut tick, profile);
    assert_eq!(rejoined.joined.inventory, fresh);
}

#[test]
fn same_profile_actions_remain_fifo_while_the_first_wal_write_is_pending() {
    let save = TestSave::new("profile-fifo");
    let profile = 42;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 2));
    save_inventory(&save, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let first_id = action_id(&state, profile, 1);
    let second_id = action_id(&state, profile, 2);
    let _ = messages(&session);

    run_tick(
        &mut state,
        &mut tick,
        vec![
            SimulationInput::Command {
                id: session.id,
                sequence: 1,
                message: ClientMessage::InventoryMove {
                    action_id: first_id,
                    from: 0,
                    to: 1,
                    count: 1,
                },
            },
            SimulationInput::Command {
                id: session.id,
                sequence: 2,
                message: ClientMessage::InventoryMove {
                    action_id: second_id,
                    from: 1,
                    to: 2,
                    count: 1,
                },
            },
        ],
    );

    assert_eq!(state.durability.pending.len(), 1);
    let queued_command_ids: Vec<_> = state
        .durability
        .queued
        .iter()
        .filter_map(|request| match request {
            DurableRequest::Command { message, .. } => match message {
                ClientMessage::Edit { action_id, .. }
                | ClientMessage::InventoryMove { action_id, .. }
                | ClientMessage::DropStack { action_id, .. } => Some(*action_id),
                _ => None,
            },
            DurableRequest::Pickup { .. }
            | DurableRequest::Expire
            | DurableRequest::EntityTick { .. }
            | DurableRequest::EntityWake { .. } => None,
        })
        .collect();
    assert_eq!(queued_command_ids, [second_id]);
    assert_eq!(
        state.clients[&session.id].inventory.slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots[0]
            .as_ref()
            .is_some_and(|stack| stack.count == 1)
            && inventory.slots[1].is_none()
            && inventory.slots[2]
                .as_ref()
                .is_some_and(|stack| stack.count == 1)
    });

    let output = messages(&session);
    assert_eq!(action_result(&output, first_id), Some(true));
    assert_eq!(action_result(&output, second_id), Some(true));
}

#[test]
fn inventory_move_is_wal_gated_exactly_retried_and_recovered() {
    let save = TestSave::new("inventory-recovery");
    let profile = 73;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 2));
    save_inventory(&save, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let move_id = action_id(&state, profile, 1);
    let _ = messages(&session);
    let command = ClientMessage::InventoryMove {
        action_id: move_id,
        from: 0,
        to: 1,
        count: 1,
    };

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: session.id,
            sequence: 1,
            message: command.clone(),
        }],
    );
    assert_eq!(
        state.clients[&session.id].inventory.slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    assert!(
        !messages(&session)
            .iter()
            .any(|message| matches!(message, ServerMessage::ActionResult { action_id, .. } if *action_id == move_id))
    );
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots[0]
            .as_ref()
            .is_some_and(|stack| stack.count == 1)
            && inventory.slots[1]
                .as_ref()
                .is_some_and(|stack| stack.count == 1)
    });
    assert_eq!(action_result(&messages(&session), move_id), Some(true));

    let exact_retry =
        command_and_wait(&mut state, &mut tick, &session, 2, move_id, command.clone());
    assert_eq!(action_result(&exact_retry, move_id), Some(true));
    assert_eq!(
        state.clients[&session.id].inventory.slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    let changed_retry = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        3,
        move_id,
        ClientMessage::InventoryMove {
            action_id: move_id,
            from: 0,
            to: 2,
            count: 1,
        },
    );
    assert_eq!(action_result(&changed_retry, move_id), Some(false));
    assert!(state.clients[&session.id].inventory.slots[2].is_none());

    drop(session);
    drop(state);

    let mut restarted = state_for(&save, 7);
    let mut restart_tick = 1;
    let restored = join(&mut restarted, &mut restart_tick, profile);
    assert_ne!(action_id(&restarted, profile, 1), move_id);
    assert_eq!(
        restored.joined.inventory.slots[0].as_ref().unwrap().count,
        1
    );
    assert_eq!(
        restored.joined.inventory.slots[1].as_ref().unwrap().count,
        1
    );
    let retry_after_restart = command_and_wait(
        &mut restarted,
        &mut restart_tick,
        &restored,
        1,
        move_id,
        command,
    );
    assert_eq!(action_result(&retry_after_restart, move_id), Some(false));
    assert_eq!(
        restarted.clients[&restored.id].inventory.slots[1]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}

#[test]
fn live_placement_harvest_and_drop_stack_survive_restart() {
    let save = TestSave::new("edit-drop-recovery");
    let profile = 84;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 2));
    save_inventory(&save, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let place_id = action_id(&state, profile, 1);
    let harvest_id = action_id(&state, profile, 2);
    let drop_id = action_id(&state, profile, 3);
    let _ = messages(&session);
    let feet_y = session.joined.position[1] as i32;
    let target_y = feet_y + 2;
    let target_key = world_to_chunk(0, target_y, 0).0;
    state
        .clients
        .get_mut(&session.id)
        .unwrap()
        .sent
        .insert(target_key);

    let placed = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        1,
        place_id,
        ClientMessage::Edit {
            action_id: place_id,
            x: 0,
            y: target_y,
            z: 0,
            block: crate::world::STONE,
            slot: 0,
        },
    );
    assert_eq!(action_result(&placed, place_id), Some(true));
    assert_eq!(
        state.world.cached_block(0, target_y, 0),
        Some(crate::world::STONE)
    );
    assert_eq!(
        state.clients[&session.id].inventory.slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );

    let harvested = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        2,
        harvest_id,
        ClientMessage::Edit {
            action_id: harvest_id,
            x: 0,
            y: target_y,
            z: 0,
            block: AIR,
            slot: 0,
        },
    );
    assert_eq!(action_result(&harvested, harvest_id), Some(true));
    assert_eq!(state.world.cached_block(0, target_y, 0), Some(AIR));
    assert_eq!(state.drops.nearby(session.joined.position).len(), 1);

    let thrown = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        3,
        drop_id,
        ClientMessage::DropStack {
            action_id: drop_id,
            slot: 0,
            count: 1,
        },
    );
    assert_eq!(action_result(&thrown, drop_id), Some(true));
    assert!(state.clients[&session.id].inventory.slots[0].is_none());
    assert_eq!(state.drops.nearby(session.joined.position).len(), 2);

    drop(session);
    drop(state);

    let mut restarted = state_for(&save, 7);
    assert_eq!(restarted.world.get_block(0, target_y, 0).unwrap(), AIR);
    let restored_inventory = restarted.inventory_store.load(profile).unwrap();
    assert!(restored_inventory.slots[0].is_none());
    assert_eq!(restarted.drops.nearby([0.5, target_y as f32, 0.5]).len(), 2);
}

#[test]
fn pickup_commits_inventory_and_drop_removal_before_restart() {
    let save = TestSave::new("pickup-recovery");
    let profile = 95;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 1));
    save_inventory(&save, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let drop_id = action_id(&state, profile, 1);
    let _ = messages(&session);

    let thrown = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        1,
        drop_id,
        ClientMessage::DropStack {
            action_id: drop_id,
            slot: 0,
            count: 1,
        },
    );
    assert_eq!(action_result(&thrown, drop_id), Some(true));
    assert!(state.clients[&session.id].inventory.slots[0].is_none());
    assert_eq!(state.drops.nearby(session.joined.position).len(), 1);

    std::thread::sleep(Duration::from_millis(1_510));
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots[0]
            .as_ref()
            .is_some_and(|stack| stack.item == STONE_ITEM && stack.count == 1)
    });
    assert!(state.drops.nearby(session.joined.position).is_empty());
    assert!(
        messages(&session)
            .iter()
            .any(|message| matches!(message, ServerMessage::Pickups { items } if items.len() == 1))
    );

    let pickup_position = session.joined.position;
    drop(session);
    drop(state);

    let restarted = state_for(&save, 7);
    let restored_inventory = restarted.inventory_store.load(profile).unwrap();
    assert_eq!(
        restored_inventory.slots[0],
        Some(crate::inventory::Stack::new(STONE_ITEM, 1))
    );
    assert!(restarted.drops.nearby(pickup_position).is_empty());
}

#[test]
fn post_cut_pickup_replays_from_an_older_checkpoint_after_full_server_restart() {
    let save = TestSave::new("closed-drop-set-lagging-pickup");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 713);
    let position = session.joined.position;

    // This starts as a legacy BGDP-only drop. The forced generation cut must
    // materialize it in the base before pruning historical owner keys.
    // Join queued the normal pickup probe while no drops existed; rotation
    // holds it until the new base is durable.
    state.durability.retry_pickups.insert(session.id);
    assert!(
        state
            .durability
            .queued
            .iter()
            .any(|request| matches!(request, DurableRequest::Pickup { id } if *id == session.id))
    );
    state.drops.spawn(position, STONE_ITEM, 1, Duration::ZERO);
    state.durability.rotation_requested = true;
    for _ in 0..2_000 {
        run_empty_tick(&mut state, &mut tick);
        if !state.durability.rotation_requested {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !state.durability.rotation_requested,
        "closed-owner-set rotation did not finish"
    );
    assert!(state.durability.dirty_checkpoints.is_empty());
    assert!(state.durability.checkpoint_inflight.is_empty());
    assert_eq!(state.drops.nearby(position).len(), 1);
    // Capture the sharded drop base (with the drop) so the test can restore
    // these exact older but checksum-valid bytes below. That recreates a
    // crash after the WAL sync but before drop checkpointing.
    let old_drop_shards = snapshot_drop_shards(&save);
    assert!(
        !old_drop_shards.is_empty(),
        "rotation must have checkpointed the drop shards"
    );

    // Commit a post-cut pickup, then allow its checkpoint to finish so the
    // test can restore the exact older but checksum-valid BGDP bytes below.
    // That recreates a crash after the WAL sync but before drop checkpointing.
    let mut completed = false;
    for _ in 0..1_000 {
        run_empty_tick(&mut state, &mut tick);
        if state.durability.pending.is_empty()
            && state.durability.dirty_checkpoints.is_empty()
            && state.durability.checkpoint_inflight.is_empty()
            && state.drops.nearby(position).is_empty()
        {
            completed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(completed, "post-cut pickup/checkpoint did not complete");
    assert_eq!(
        state.clients[&session.id].inventory.slots[0],
        Some(crate::inventory::Stack::new(STONE_ITEM, 1))
    );
    restore_drop_shards(&save, &old_drop_shards);

    drop(session);
    drop(state);
    let restarted = state_for(&save, 7);
    assert!(restarted.drops.nearby(position).is_empty());
    assert_eq!(
        restarted.inventory_store.load(713).unwrap().slots[0],
        Some(crate::inventory::Stack::new(STONE_ITEM, 1))
    );
    let recovered_drops = Drops::open(save.path()).unwrap();
    assert!(recovered_drops.nearby(position).is_empty());
}

#[test]
fn drops_conserve_items_across_chunk_transfer_settle_and_restart() {
    let save = TestSave::new("drop-transfer-conservation");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    // Far above the surface so the fall crosses chunk y-boundaries on the
    // way down: every crossing is an atomic owner transfer.
    state.drops.spawn(
        [0.5, surface_y as f32 + 64.0, 0.5],
        STONE_ITEM,
        200,
        Duration::ZERO,
    );
    let mut tick = 1;
    for _ in 0..2_000 {
        run_empty_tick(&mut state, &mut tick);
        if state.drops.active_len() == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(state.drops.active_len(), 0);
    let settled = state.drops.nearby([0.5, surface_y as f32, 0.5]);
    assert_eq!(
        settled
            .iter()
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        200
    );
    let before: Vec<(u64, u16, [f32; 3])> = settled
        .iter()
        .map(|drop| (drop.id, drop.count, drop.position))
        .collect();
    state.drops.save().unwrap();
    // The 64-block fall crossed chunk y-boundaries: the settled owner
    // differs from the spawn owner, so atomic transfers ran mid-fall.
    let spawn_owner = crate::world::world_to_chunk(0, surface_y + 64, 0).0;
    let settled_owner = crate::world::world_to_chunk(
        0,
        settled[0].position[1].floor() as i32,
        0,
    )
    .0;
    assert_ne!(
        spawn_owner.y, settled_owner.y,
        "the fall must span chunk owners"
    );
    // The checkpoint path persisted the landing shard.
    assert!(
        fs::read_dir(save.path().join("drops.d"))
            .unwrap()
            .flatten()
            .any(|entry| {
                entry.file_name().to_string_lossy().starts_with("chunk_")
            }),
        "settle must checkpoint a drop shard"
    );
    drop(state);

    let restarted = state_for(&save, 7);
    let after = restarted.drops.nearby([0.5, surface_y as f32, 0.5]);
    assert_eq!(
        after
            .iter()
            .map(|drop| (drop.id, drop.count, drop.position))
            .collect::<Vec<_>>(),
        before,
        "chunk transfer and restart preserve every drop identically"
    );
    assert_eq!(
        after
            .iter()
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        200
    );
}
