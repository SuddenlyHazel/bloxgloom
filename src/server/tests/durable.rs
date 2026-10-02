use super::*;

const STONE_ITEM: crate::items::ItemId = crate::items::ItemId::new(crate::world::STONE.get());

#[test]
fn dense_drop_page_advances_through_production_dispatch() {
    let save = TestSave::new("dense-drop-physics");
    let mut state = state_for(&save, 7);
    let y = crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0;
    reside_neighbourhood(&mut state, [0.5, y, 0.5]);
    for z in 0..8 {
        for x in 0..8 {
            spawn_drop(
                &mut state,
                1,
                [0.5 + 2.0 * x as f32, y, 0.5 + 2.0 * z as f32],
                STONE_ITEM,
                1,
                Duration::ZERO,
            );
        }
    }
    spawn_drop(
        &mut state,
        1,
        [1.5, y + 2.0, 1.5],
        STONE_ITEM,
        1,
        Duration::ZERO,
    );
    let ids = state.entities.due_entities(2, 128);
    assert_eq!(ids.len(), 65);
    let input =
        crate::server::durable::actions::entity::capture_tick_input(&mut state, ids[0], 2, false)
            .unwrap()
            .unwrap();
    assert_eq!(
        input.neighbours.len(),
        0,
        "physics declares no entity reads"
    );
    for id in &ids {
        state
            .durability
            .queued
            .push_back(DurableRequest::EntityTick { id: *id });
    }
    crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
        .unwrap();
    assert!(
        ids.iter()
            .all(|id| state.entities.snapshot(*id).unwrap().motion_revision > 0)
    );
}

#[test]
fn queued_wakes_cannot_fill_the_due_entity_lane() {
    let save = TestSave::new("wake-pressure-due-drop");
    let mut state = state_for(&save, 7);
    let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    reside_neighbourhood(&mut state, position);
    spawn_drop(&mut state, 1, position, STONE_ITEM, 1, Duration::ZERO);
    let id = state.entities.due_entities(2, 1)[0];
    for _ in 0..crate::server::durable::MAX_DEFERRED_DURABLE_ACTIONS {
        state
            .durability
            .queued
            .push_back(DurableRequest::EntityWake { id });
    }
    crate::server::durable::queue_interaction_actions(&mut state, TickId::new(2));
    assert!(state.durability.queued.iter().any(
        |request| matches!(request, DurableRequest::EntityTick { id: queued } if *queued == id)
    ));
    crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
        .unwrap();
    assert!(state.entities.snapshot(id).unwrap().motion_revision > 0);
}

#[test]
fn stale_entity_worker_capture_does_not_prepare_or_apply_and_remains_retryable() {
    let save = TestSave::new("stale-entity-worker-capture");
    let mut state = state_for(&save, 7);
    let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    reside_neighbourhood(&mut state, position);
    spawn_drop(&mut state, 1, position, STONE_ITEM, 1, Duration::ZERO);
    let id = state.entities.due_entities(2, 1)[0];
    let input =
        crate::server::durable::actions::entity::capture_tick_input(&mut state, id, 2, false)
            .unwrap()
            .unwrap();
    let plan = input.plan().unwrap();
    let before = state.entities.snapshot(id).unwrap();
    // Physics declares no neighbour reads. Change an actual captured terrain
    // dependency, not an unrelated entity's publication watermark.
    let edit = state
        .world
        .prepare_edits(&[(0, position[1] as i32 - 1, 0, crate::world::STONE)])
        .unwrap();
    state.world.apply_prepared_edits(edit).unwrap();
    let error = crate::server::durable::actions::entity::commit_tick_plan(&mut state, input, plan)
        .err()
        .expect("stale capture cannot construct a transaction");
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(
        state.entities.snapshot(id).unwrap().revision,
        before.revision
    );
    state
        .durability
        .queued
        .push_back(DurableRequest::EntityTick { id });
    crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
        .unwrap();
    assert!(state.entities.snapshot(id).unwrap().motion_revision > before.motion_revision);
    assert!(state.durability.pending.is_empty());
}

#[test]
fn outstanding_terrain_edit_defers_drop_motion_until_replanned_single_and_batched() {
    use crate::server::durable::actions::entity::plan_entity_tick;

    for count in [1, 2] {
        let save = TestSave::new("terrain-fenced-drop-motion");
        let mut state = state_for(&save, 7);
        let y = crate::world::MAX_GENERATED_HEIGHT + 20;
        let positions = [[0.5, y as f32, 0.5], [2.5, y as f32, 0.5]];
        reside_neighbourhood(&mut state, positions[0]);
        for position in positions.iter().take(count) {
            spawn_drop(&mut state, 1, *position, STONE_ITEM, 1, Duration::ZERO);
        }
        let ids: Vec<_> = positions
            .iter()
            .take(count)
            .map(|position| {
                crate::server::entities::EntityId::new(
                    drop_nearby(&state, *position)
                        .into_iter()
                        .find(|drop| drop.position == *position)
                        .expect("spawned drop exists at its position")
                        .id,
                )
                .unwrap()
            })
            .collect();
        let before: Vec<_> = ids
            .iter()
            .map(|id| state.entities.snapshot(*id).unwrap())
            .collect();
        let cell = (0, y - 1, 0);
        assert_eq!(
            state.world.cached_block(cell.0, cell.1, cell.2),
            Some(crate::world::AIR)
        );
        let edits = state
            .world
            .prepare_edits(&[(cell.0, cell.1, cell.2, crate::world::STONE)])
            .unwrap();
        let terrain = crate::server::durable::CommitAction {
            client_id: None,
            profile: None,
            action_id: None,
            receipt_value: None,
            receipt_transition: None,
            terrain_reads: Default::default(),
            inventory_before: None,
            inventory: None,
            world_edits: edits,
            deltas: Vec::new(),
            changed_cells: Vec::new(),
            pickups: Vec::new(),
            fire_seed: None,
            clock_change: None,
            weather_change: None,
            entities: None,
            entity_wakes: Vec::new(),
            owner_changes: vec![],
            sounds: Vec::new(),
            player_publication: None,
        };
        assert!(
            state
                .durability
                .try_stage(TickId::new(2), &terrain, None)
                .unwrap()
        );
        let terrain_key = crate::server::durable::chunk_state_key(
            crate::world::world_to_chunk(cell.0, cell.1, cell.2).0,
        );
        assert!(state.durability.reserved.contains(&terrain_key));

        // Hold the receipt at the coordinator boundary even if the writer has
        // finished: the edit is reserved but has not changed resident terrain.
        let (sender, withheld) = mpsc::channel();
        let real_receipt = std::mem::replace(&mut state.durability.pending[0].receiver, withheld);
        for id in &ids {
            let action = plan_entity_tick(&mut state, *id, 2, false)
                .unwrap()
                .expect("drop is due over the original air");
            assert!(
                action
                    .entities
                    .as_ref()
                    .unwrap()
                    .read_keys()
                    .any(|key| key == &terrain_key)
            );
        }
        for id in &ids {
            state
                .durability
                .queued
                .push_back(DurableRequest::EntityTick { id: *id });
        }
        crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
            .unwrap();
        assert_eq!(
            state.durability.pending.len(),
            1,
            "no partial motion may stage"
        );
        assert_eq!(state.durability.queued.len(), count, "all steps must retry");
        for (id, original) in ids.iter().zip(&before) {
            assert_eq!(
                state.entities.snapshot(*id).unwrap().motion_revision,
                original.motion_revision
            );
        }

        // Now admit the real terrain receipt; a fresh tick must plan against
        // stone, not apply the stale air-based positions held above.
        drop(sender);
        state.durability.pending[0].receiver = real_receipt;
        drain_durable(&mut state, 2);
        assert_eq!(
            state.world.cached_block(cell.0, cell.1, cell.2),
            Some(crate::world::STONE)
        );
        let expected: Vec<_> = ids
            .iter()
            .map(|id| {
                plan_entity_tick(&mut state, *id, 2, false)
                    .unwrap()
                    .unwrap()
                    .entities
                    .unwrap()
            })
            .collect();
        if count == 2 {
            let batch = state.entities.combine_prepared(expected).unwrap();
            assert!(
                batch.read_keys().any(|key| key == &terrain_key),
                "batch keeps terrain reads"
            );
        }
        crate::server::durable::process_durable_actions(&mut state, TickId::new(2), Instant::now())
            .unwrap();
        assert!(state.durability.pending.is_empty());
        assert!(state.durability.queued.is_empty());
        assert!(
            state.entities.snapshot(ids[0]).unwrap().motion_revision > before[0].motion_revision
        );
        // The edited block catches the first drop; without replanning it
        // would have fallen through the new top on its stale air trajectory.
        let crate::server::entities::EntityLocation::Mobile { position: first } =
            state.entities.snapshot(ids[0]).unwrap().location
        else {
            panic!("drop remains mobile");
        };
        assert!(first[1] > positions[0][1]);
        if count == 2 {
            assert!(
                state.entities.snapshot(ids[1]).unwrap().motion_revision
                    > before[1].motion_revision
            );
            let crate::server::entities::EntityLocation::Mobile { position: second } =
                state.entities.snapshot(ids[1]).unwrap().location
            else {
                panic!("drop remains mobile");
            };
            assert!(second[1] < positions[1][1]);
        }
    }
}

fn action_id(state: &State, profile: u128, seq: u64) -> u128 {
    (u128::from(state.durability.receipt_ledger(profile).current_epoch()) << 64) | u128::from(seq)
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
    save_inventory(&save, 7, profile, &inventory);
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
            guard: crate::server::players::JoinGuard::default(),
            name: format!("player-{profile:x}"),
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
    save_inventory(&save, 7, profile, &inventory);
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
            | DurableRequest::Ecology { .. }
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
    save_inventory(&save, 7, profile, &inventory);
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
    save_inventory(&save, 7, profile, &inventory);
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
    wait_for_subscription(&mut state, &mut tick, session.id, target_key);

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
    assert_eq!(drop_nearby(&state, session.joined.position).len(), 1);

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
    assert_eq!(drop_nearby(&state, session.joined.position).len(), 2);

    drop(session);
    drop(state);

    let mut restarted = state_for(&save, 7);
    assert_eq!(restarted.world.get_block(0, target_y, 0).unwrap(), AIR);
    let restored_inventory = restarted.inventory_store.load(profile).unwrap();
    assert!(restored_inventory.slots[0].is_none());
    assert_eq!(
        drop_nearby(&restarted, [0.5, target_y as f32, 0.5]).len(),
        2
    );
}

#[test]
fn pickup_commits_inventory_and_drop_removal_before_restart() {
    let save = TestSave::new("pickup-recovery");
    let profile = 95;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 1));
    save_inventory(&save, 7, profile, &inventory);
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
    assert_eq!(drop_nearby(&state, session.joined.position).len(), 1);

    std::thread::sleep(Duration::from_millis(1_510));
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots[0]
            .as_ref()
            .is_some_and(|stack| stack.item == STONE_ITEM && stack.count == 1)
    });
    assert!(drop_nearby(&state, session.joined.position).is_empty());
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
    assert!(drop_nearby(&restarted, pickup_position).is_empty());
}

#[test]
fn post_cut_pickup_replays_from_an_older_checkpoint_after_full_server_restart() {
    let save = TestSave::new("closed-drop-set-lagging-pickup");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 713);
    let position = session.joined.position;

    // A WAL-staged spawn followed by a forced generation cut: the BGEN
    // checkpoint now covers the drop, so everything after replays from WAL.
    spawn_drop(&mut state, tick, position, STONE_ITEM, 1, Duration::ZERO);
    // Keep pickup in the post-cut phase deliberately. A tick that completes
    // rotation may also admit and apply new work; relying on async receipt
    // timing here allowed pickup to race the checkpoint assertions.
    let client = state.clients.remove(&session.id).unwrap();
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
        "rotation did not finish"
    );
    assert!(state.durability.dirty_checkpoints.is_empty());
    assert!(state.durability.checkpoint_inflight.is_empty());
    assert_eq!(drop_nearby(&state, position).len(), 1);

    state.clients.insert(session.id, client);

    // Commit a post-cut pickup and let its WAL receipt apply — but do NOT
    // rotate again. Restart must replay the pickup from the journal over
    // the older checkpoint that still contains the drop.
    let mut completed = false;
    for _ in 0..1_000 {
        run_empty_tick(&mut state, &mut tick);
        if state.durability.pending.is_empty() && drop_nearby(&state, position).is_empty() {
            completed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(completed, "post-cut pickup did not complete");
    assert_eq!(
        state.clients[&session.id].inventory.slots[0],
        Some(crate::inventory::Stack::new(STONE_ITEM, 1))
    );

    drop(session);
    drop(state);
    let restarted = state_for(&save, 7);
    assert!(drop_nearby(&restarted, position).is_empty());
    assert_eq!(
        restarted.inventory_store.load(713).unwrap().slots[0],
        Some(crate::inventory::Stack::new(STONE_ITEM, 1))
    );
}

#[test]
fn crash_mid_pickup_cannot_lose_or_duplicate_the_item() {
    let save = TestSave::new("mid-pickup-crash");
    let profile = 1551;
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let at = session.joined.position;
    // One stone on the ground, immediately pickable. No ticks run after the
    // spawn, so the automatic pickup probe cannot beat the manual staging.
    spawn_drop(&mut state, tick, at, STONE_ITEM, 1, Duration::ZERO);
    // Plan the real pickup and sync it to the journal, then crash before
    // the coordinator ever applies it.
    let request = DurableRequest::Pickup { id: session.id };
    let action = crate::server::durable::actions::plan_durable_request(
        &mut state,
        &request,
        TickId::new(tick),
    )
    .unwrap()
    .expect("pickup plans while the drop is down");
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the pickup");
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), &action, Some(permit))
            .unwrap()
    );
    state.durability.pending[0]
        .receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    drop(session);
    drop(state);
    // Recovery replays the synced-but-unapplied pickup atomically: the stone
    // reaches the inventory exactly once and no drop remains.
    let restarted = state_for(&save, 7);
    let inventory = restarted.inventory_store.load(profile).unwrap();
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| u32::from(stack.count))
            .sum::<u32>(),
        1
    );
    assert!(drop_nearby(&restarted, at).is_empty());
}

#[test]
fn stacking_pickup_and_restart_conserve_every_item() {
    let save = TestSave::new("stacking-conservation");
    let profile = 1552;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(STONE_ITEM, 100));
    save_inventory(&save, 7, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let at = session.joined.position;
    // 100 more stone on the ground stacks into the half-full inventory
    // through the normal proximity pickup: 128 + 72, nothing created.
    spawn_drop(&mut state, tick, at, STONE_ITEM, 100, Duration::ZERO);
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| u32::from(stack.count))
            .sum::<u32>()
            == 200
    });
    assert!(drop_nearby(&state, at).is_empty());
    drop(session);
    drop(state);
    let restarted = state_for(&save, 7);
    let inventory = restarted.inventory_store.load(profile).unwrap();
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| u32::from(stack.count))
            .sum::<u32>(),
        200
    );
    assert!(drop_nearby(&restarted, at).is_empty());
}

#[test]
fn pickup_leaves_the_exact_uncredited_remainder_after_restart() {
    let save = TestSave::new("partial-pickup-conservation");
    let profile = 1554;
    let mut inventory = Inventory::default();
    for slot in &mut inventory.slots {
        *slot = Some(crate::inventory::Stack::new(STONE_ITEM, 128));
    }
    inventory.slots[0].as_mut().unwrap().count = 127;
    save_inventory(&save, 7, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let position = session.joined.position;
    spawn_drop(&mut state, tick, position, STONE_ITEM, 5, Duration::ZERO);
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots[0]
            .as_ref()
            .is_some_and(|stack| stack.count == 128)
    });
    let dropped = drop_nearby(&state, position);
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].count, 4);
    drop(session);
    drop(state);
    let restarted = state_for(&save, 7);
    assert_eq!(
        restarted.inventory_store.load(profile).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        128
    );
    let dropped = drop_nearby(&restarted, position);
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].count, 4);
}

#[test]
fn presentation_timing_cannot_create_or_remove_drops() {
    let save = TestSave::new("presentation-isolation");
    let profile = 1553;
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    // One stone settles far outside pickup range; the idle client sends
    // nothing for the rest of the test, so only server ticks run.
    let surface_y = state.spawn_anchor[1] as i32;
    let far = [
        session.joined.position[0] + 40.0,
        surface_y as f32 + 0.25,
        session.joined.position[2],
    ];
    spawn_drop(&mut state, tick, far, STONE_ITEM, 7, Duration::ZERO);
    for _ in 0..2_000 {
        run_empty_tick(&mut state, &mut tick);
        if drop_active_len(&state) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&state), 0);
    let before = drop_nearby(&state, far);
    assert_eq!(before.len(), 1);
    let _ = messages(&session);
    for _ in 0..20 {
        run_empty_tick(&mut state, &mut tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Server ticks alone change nothing: same drop, same position, and no
    // pickup event without proximity. Ages advance with the server clock,
    // so only ownership fields compare.
    let owned = |items: &[crate::protocol::DroppedItem]| {
        items
            .iter()
            .map(|drop| (drop.id, drop.item, drop.count, drop.position))
            .collect::<Vec<_>>()
    };
    assert_eq!(owned(&drop_nearby(&state, far)), owned(&before));
    assert!(
        messages(&session)
            .iter()
            .all(|message| !matches!(message, ServerMessage::Pickups { .. }))
    );
    drop(session);
}

#[test]
fn mid_fall_crash_conserves_every_item() {
    let save = TestSave::new("mid-fall-crash");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    let mut tick = 1;
    spawn_drop(
        &mut state,
        tick,
        [0.5, surface_y as f32 + 64.0, 0.5],
        STONE_ITEM,
        200,
        Duration::ZERO,
    );
    for _ in 0..150 {
        run_empty_tick(&mut state, &mut tick);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Still falling: nowhere near settled, so the crash lands mid-transfer.
    assert!(drop_active_len(&state) > 0);
    drop(state);
    // Every item survives the crash mid-flight: 200 across live drops,
    // none created, none lost.
    let restarted = state_for(&save, 7);
    let after = drop_nearby(&restarted, [0.5, surface_y as f32 + 32.0, 0.5]);
    assert!(!after.is_empty());
    assert_eq!(
        after.iter().map(|drop| u32::from(drop.count)).sum::<u32>(),
        200
    );
}

#[test]
fn drops_conserve_items_across_chunk_transfer_settle_and_restart() {
    let save = TestSave::new("drop-transfer-conservation");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    // Far above the surface so the fall crosses chunk y-boundaries on the
    // way down: every crossing is an atomic owner transfer.
    let mut tick = 1;
    spawn_drop(
        &mut state,
        tick,
        [0.5, surface_y as f32 + 64.0, 0.5],
        STONE_ITEM,
        200,
        Duration::ZERO,
    );
    // The 64-block fall needs ~140 staged motion steps (one WAL receipt
    // each), so this loop budgets wall-clock time rather than tick counts.
    for _ in 0..6_000 {
        run_empty_tick(&mut state, &mut tick);
        if drop_active_len(&state) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&state), 0);
    let settled = drop_nearby(&state, [0.5, surface_y as f32, 0.5]);
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
    // The 64-block fall crossed chunk y-boundaries: the settled owner
    // differs from the spawn owner, so atomic transfers ran mid-fall. The
    // owner below is the entity's authoritative chunk owner, not a position
    // derived guess.
    let spawn_owner = crate::world::world_to_chunk(0, surface_y + 64, 0).0;
    let settled_id =
        crate::server::entities::EntityId::new(settled[0].id).expect("settled drop has an ID");
    let settled_owner = match state
        .entities
        .snapshot(settled_id)
        .expect("settled drop exists")
        .owner
    {
        crate::server::entities::EntityOwner::Mobile(chunk) => chunk,
        owner => panic!("drops stay mobile, found {owner:?}"),
    };
    assert_ne!(
        spawn_owner.y, settled_owner.y,
        "the fall must span chunk owners"
    );
    drop(state);

    let restarted = state_for(&save, 7);
    let after = drop_nearby(&restarted, [0.5, surface_y as f32, 0.5]);
    assert_eq!(
        after
            .iter()
            .map(|drop| (drop.id, drop.count, drop.position))
            .collect::<Vec<_>>(),
        before,
        "chunk transfer and restart preserve every drop identically"
    );
    assert_eq!(
        after.iter().map(|drop| u32::from(drop.count)).sum::<u32>(),
        200
    );
}

#[test]
fn drop_trajectory_is_identical_with_and_without_receipt_drains() {
    type DropKey = (u64, crate::items::ItemId, u16, [f32; 3]);
    type DropRest = (u64, u16, [f32; 3]);
    // Phase 1 runs inside the synchronously resided spawn column, so no
    // chunk-loader wall time can intervene: any per-tick difference here is
    // receipt timing, not terrain streaming. Phase 2 settles with loader
    // sleeps and compares only the final rest.
    fn run(drained: bool) -> (Vec<DropKey>, DropRest) {
        let save = TestSave::new("drop-timing-proof");
        let mut state = state_for(&save, 7);
        let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
        reside_neighbourhood(&mut state, position);
        // The falling entity enters the next chunk down during this window.
        // Its declared view needs that chunk's lower neighbour too; pin both
        // neighbourhoods so loader timing cannot masquerade as receipt timing.
        reside_neighbourhood(&mut state, [position[0], position[1] - 16.0, position[2]]);
        spawn_drop(&mut state, 1, position, STONE_ITEM, 1, Duration::ZERO);
        let mut sequence = Vec::new();
        for tick in 1..120u64 {
            tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
            if drained {
                drain_durable(&mut state, tick);
            } else {
                // No drain and no sleep: the tick itself must have committed
                // the motion, so no receipt may still be outstanding.
                assert!(
                    state.durability.pending.is_empty(),
                    "tick {tick} left staged motion waiting on a receipt"
                );
            }
            let nearby = drop_nearby(&state, position);
            assert_eq!(nearby.len(), 1);
            sequence.push((
                nearby[0].id,
                nearby[0].item,
                nearby[0].count,
                nearby[0].position,
            ));
        }
        let mut center = position;
        for tick in 120..2_120 {
            tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
            let nearby = drop_nearby(&state, center);
            assert_eq!(nearby.len(), 1);
            center = nearby[0].position;
            if drop_active_len(&state) == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(drop_active_len(&state), 0, "drop must settle");
        let rest = drop_nearby(&state, center);
        assert_eq!(rest.len(), 1);
        (sequence, (rest[0].id, rest[0].count, rest[0].position))
    }

    let (tight, tight_rest) = run(false);
    let (drained, drained_rest) = run(true);
    assert!(
        tight.len() == 119,
        "the resident window must run its full length"
    );
    assert_eq!(tight, drained, "receipt timing changed the drop trajectory");
    assert_eq!(tight_rest, drained_rest, "receipt timing changed the rest");
}

#[test]
fn staggered_drop_merge_resolves_identically_under_receipt_timing() {
    type DropKey = (u64, u16, [f32; 3]);
    // Same two phases as the single-drop proof: a tick-exact resident
    // window holding the staggered merge, then settling to compare rest.
    fn run(drained: bool) -> (Vec<Vec<DropKey>>, Vec<DropKey>) {
        let save = TestSave::new("drop-merge-timing");
        let mut state = state_for(&save, 7);
        let top = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
        reside_neighbourhood(&mut state, top);
        // Keep the complete declared view resident after crossing downward.
        reside_neighbourhood(&mut state, [top[0], top[1] - 16.0, top[2]]);
        spawn_drop(&mut state, 1, top, STONE_ITEM, 30, Duration::ZERO);
        let mut sequence = Vec::new();
        for tick in 1..120u64 {
            if tick == 6 {
                // The first drop is already falling: this spawn merges into
                // it only when the applied trajectory says they overlap.
                spawn_drop(&mut state, tick, top, STONE_ITEM, 40, Duration::ZERO);
                let merged = drop_nearby(&state, top);
                assert_eq!(merged.len(), 1, "staggered spawn must merge mid-fall");
                assert_eq!(
                    merged.iter().map(|drop| u32::from(drop.count)).sum::<u32>(),
                    70
                );
            }
            tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
            if drained {
                drain_durable(&mut state, tick);
            }
            let mut settled: Vec<(u64, u16, [f32; 3])> = drop_nearby(&state, top)
                .iter()
                .map(|drop| (drop.id, drop.count, drop.position))
                .collect();
            settled.sort_by_key(|(id, count, position)| (*id, *count, position.map(f32::to_bits)));
            sequence.push(settled);
        }
        let mut center = top;
        for tick in 120..2_120 {
            tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
            let nearby = drop_nearby(&state, center);
            assert_eq!(nearby.len(), 1);
            center = nearby[0].position;
            if drop_active_len(&state) == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(drop_active_len(&state), 0, "drops must settle");
        let rest: Vec<(u64, u16, [f32; 3])> = drop_nearby(&state, center)
            .iter()
            .map(|drop| (drop.id, drop.count, drop.position))
            .collect();
        assert_eq!(rest.iter().map(|drop| u32::from(drop.1)).sum::<u32>(), 70);
        (sequence, rest)
    }

    let (tight, tight_rest) = run(false);
    let (drained, drained_rest) = run(true);
    assert_eq!(
        tight, drained,
        "receipt timing changed the meeting-drop merge outcome"
    );
    assert_eq!(tight_rest, drained_rest, "receipt timing changed the rest");
}

#[test]
fn withheld_motion_receipt_defers_without_duplicating_or_losing_steps() {
    use crate::server::durable::StageError;
    use crate::server::durable::actions::entity::plan_entity_tick;

    fn setup(label: &str) -> (TestSave, State, [f32; 3]) {
        let save = TestSave::new(label);
        let mut state = state_for(&save, 7);
        let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
        reside_neighbourhood(&mut state, position);
        spawn_drop(&mut state, 1, position, STONE_ITEM, 7, Duration::ZERO);
        (save, state, position)
    }

    let (_save, mut reference, position) = setup("drop-withhold-ref");
    let (_save, mut delayed, _) = setup("drop-withhold-delayed");
    // Advance both identically so the drop is falling with no receipts
    // outstanding; the reference then runs ahead on plain ticks.
    for tick in 1..=4u64 {
        tick_once(&mut reference, TickId::new(tick), Instant::now()).unwrap();
        tick_once(&mut delayed, TickId::new(tick), Instant::now()).unwrap();
    }
    assert!(reference.durability.pending.is_empty());
    assert!(delayed.durability.pending.is_empty());
    let id = crate::server::entities::EntityId::new(drop_nearby(&delayed, position)[0].id).unwrap();

    // Stage one motion step by hand and withhold its receipt: planning the
    // next tick from still-unapplied state must not stage a second record.
    let mut staged_tick = None;
    for tick in 5..12u64 {
        if let Some(action) = plan_entity_tick(&mut delayed, id, tick, false).unwrap() {
            let permit = delayed
                .durability
                .entity_mirror
                .try_reserve_durable()
                .unwrap()
                .expect("mirror admits the withheld motion");
            assert!(
                delayed
                    .durability
                    .try_stage(TickId::new(tick), &action, Some(permit))
                    .unwrap()
            );
            staged_tick = Some(tick);
            break;
        }
    }
    let staged_tick = staged_tick.expect("a falling drop has due motion");
    assert_eq!(delayed.durability.pending.len(), 1);
    let retry = plan_entity_tick(&mut delayed, id, staged_tick + 1, false)
        .unwrap()
        .expect("applied state still shows the drop due");
    let permit = delayed
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the retry probe");
    assert!(
        matches!(
            delayed
                .durability
                .try_stage(TickId::new(staged_tick + 1), &retry, Some(permit)),
            Err(StageError::Conflict)
        ),
        "overlapping motion must defer as a conflict, never fail closed"
    );
    delayed
        .entities
        .cancel_prepared(retry.entities.as_ref().unwrap());
    assert_eq!(
        delayed.durability.pending.len(),
        1,
        "the withheld tick stages exactly one WAL record"
    );

    // Release the receipt: the withheld step lands exactly one tick later
    // than the reference's copy of the same step — delayed, never
    // duplicated or lost — and both runs settle on the same rest.
    drain_durable(&mut delayed, staged_tick);
    assert!(delayed.durability.pending.is_empty());
    let snapshot_at = |state: &State| {
        let mut settled: Vec<(u64, u16, [f32; 3])> = drop_nearby(state, position)
            .iter()
            .map(|drop| (drop.id, drop.count, drop.position))
            .collect();
        settled.sort_by_key(|(id, count, position)| (*id, *count, position.map(f32::to_bits)));
        settled
    };
    let mut reference_seq = Vec::new();
    for tick in 5..125u64 {
        tick_once(&mut reference, TickId::new(tick), Instant::now()).unwrap();
        reference_seq.push(snapshot_at(&reference));
    }
    // The withheld application shifts the schedule grid by exactly the
    // withheld window: every delayed state matches the reference one tick
    // later, with nothing skipped and nothing repeated. This window stays
    // inside the resided column so the loader cannot intervene.
    let shift = (staged_tick + 1 - 5) as usize;
    let mut delayed_seq = vec![snapshot_at(&delayed)];
    for tick in (staged_tick + 1)..(staged_tick + 120 - shift as u64) {
        tick_once(&mut delayed, TickId::new(tick), Instant::now()).unwrap();
        delayed_seq.push(snapshot_at(&delayed));
    }
    assert_eq!(
        delayed_seq.len() + shift,
        reference_seq.len(),
        "withheld receipts must shift, not reshape, the trajectory"
    );
    for (index, snapshot) in delayed_seq.iter().enumerate() {
        assert_eq!(
            *snapshot,
            reference_seq[index + shift],
            "delayed receipts diverged at delayed index {index}"
        );
    }
    // Past the resident window both runs settle with loader sleeps and rest
    // identically with every item conserved. The query center follows the
    // drop down so the 64-block view radius never clips the comparison.
    let mut reference_center = position;
    for reference_tick in 125..2_125 {
        tick_once(&mut reference, TickId::new(reference_tick), Instant::now()).unwrap();
        let nearby = drop_nearby(&reference, reference_center);
        assert_eq!(nearby.len(), 1);
        reference_center = nearby[0].position;
        if drop_active_len(&reference) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut delayed_center = position;
    for delayed_tick in (staged_tick + 120 - shift as u64)..(staged_tick + 2_120 - shift as u64) {
        tick_once(&mut delayed, TickId::new(delayed_tick), Instant::now()).unwrap();
        let nearby = drop_nearby(&delayed, delayed_center);
        assert_eq!(nearby.len(), 1);
        delayed_center = nearby[0].position;
        if drop_active_len(&delayed) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&reference), 0);
    assert_eq!(drop_active_len(&delayed), 0);
    let reference_rest = drop_nearby(&reference, reference_center);
    let delayed_rest = drop_nearby(&delayed, delayed_center);
    assert_eq!(reference_rest.len(), 1);
    assert_eq!(
        (
            delayed_rest[0].id,
            delayed_rest[0].count,
            delayed_rest[0].position
        ),
        (
            reference_rest[0].id,
            reference_rest[0].count,
            reference_rest[0].position
        )
    );
}

#[test]
fn hot_chunk_motion_is_bounded_conserved_and_settles() {
    use crate::server::drops::DropEntityPayload;
    use crate::server::entities::EntityLocation;
    // Store-level observation: the `nearby` query truncates at 256, so a
    // 260-drop population is counted straight from the entity records.
    fn live_drops(state: &State) -> Vec<(crate::server::entities::EntityId, [f32; 3], u16)> {
        let mut drops = Vec::new();
        for record in state.entities.record_values() {
            if record.entity_type != crate::server::drops::DROP_ENTITY_TYPE {
                continue;
            }
            let Some(snapshot) = state.entities.snapshot(record.id) else {
                continue;
            };
            let EntityLocation::Mobile { position } = snapshot.location else {
                continue;
            };
            let count = snapshot
                .private_payload
                .downcast_ref::<DropEntityPayload>()
                .map(|payload| payload.stack.count)
                .unwrap();
            drops.push((record.id, position, count));
        }
        drops.sort_by_key(|(id, _, _)| id.get());
        drops
    }

    let save = TestSave::new("drop-hot-chunk");
    let mut state = state_for(&save, 7);
    // Five 52-drop clusters, three chunks apart: every drop's 27-chunk
    // neighbour box holds only its own cluster (52 < 64), while 260 dues
    // exceed the 256 per-tick motion budget. Two-block spacing keeps every
    // spawn allocated instead of merged.
    let height = crate::world::MAX_GENERATED_HEIGHT as f32 + 10.0;
    let mut spots = Vec::new();
    for cluster in 0..5u64 {
        let origin = cluster as f32 * 48.0;
        for index in 0..52u64 {
            spots.push([
                origin + (index % 8) as f32 * 2.0 + 0.5,
                height,
                (index / 8) as f32 * 2.0 + 0.5,
            ]);
        }
    }
    for spot in &spots {
        reside_neighbourhood(&mut state, *spot);
    }
    for spot in &spots {
        spawn_drop(&mut state, 1, *spot, STONE_ITEM, 1, Duration::ZERO);
    }
    let before = live_drops(&state);
    assert_eq!(before.len(), 260, "every hot-chunk spawn must allocate");

    // Spawns schedule their first step for tick 2, so tick 1 idles, tick 2
    // queues the bounded due set, tick 3 stages and synchronously applies
    // it, and tick 4 carries the deferred remainder. Bounded work defers;
    // it never piles into one tick and never drops a drop.
    let mut tick = 1;
    tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
    tick += 1;
    tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
    tick += 1;
    assert_eq!(
        state
            .durability
            .queued
            .iter()
            .filter(|request| matches!(
                request,
                crate::server::durable::DurableRequest::EntityTick { .. }
            ))
            .count(),
        crate::server::durable::MAX_DURABLE_LANE_ACTIONS,
        "one tick queues at most the bounded motion budget"
    );
    tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
    tick += 1;
    assert!(state.durability.pending.is_empty());
    let after = live_drops(&state);
    assert_eq!(after.len(), 260, "bounded work never loses a drop");
    let before_set: std::collections::HashSet<[u32; 3]> = before
        .iter()
        .map(|(_, position, _)| position.map(f32::to_bits))
        .collect();
    let moved = after
        .iter()
        .filter(|(_, position, _)| !before_set.contains(&position.map(f32::to_bits)))
        .count();
    assert_eq!(
        moved,
        crate::server::durable::MAX_DURABLE_LANE_ACTIONS,
        "the whole bounded budget applies in its tick, no more"
    );
    assert_eq!(
        after
            .iter()
            .map(|(_, _, count)| u32::from(*count))
            .sum::<u32>(),
        260
    );
    assert!(
        after
            .iter()
            .all(|(_, position, _)| position.iter().all(|coordinate| coordinate.is_finite()))
    );

    for _ in 0..6_000 {
        tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
        tick += 1;
        if drop_active_len(&state) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&state), 0, "every drop settles");
    let settled = live_drops(&state);
    assert_eq!(settled.len(), 260);
    assert_eq!(
        settled
            .iter()
            .map(|(_, _, count)| u32::from(*count))
            .sum::<u32>(),
        260
    );
    assert!(settled.iter().all(|(_, _, count)| *count <= 128));
}

#[test]
fn crash_around_tick_motion_recovers_whole_never_half_moved() {
    use crate::server::durable::actions::entity::plan_entity_tick;

    fn setup(label: &str) -> (TestSave, State, u64, [f32; 3]) {
        let save = TestSave::new(label);
        let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
        let mut state = state_for(&save, 7);
        reside_neighbourhood(&mut state, position);
        spawn_drop(&mut state, 1, position, STONE_ITEM, 7, Duration::ZERO);
        (save, state, 1, position)
    }

    // The uninterrupted trajectory: every position below is a whole record.
    // Settling takes loader sleeps past the resided column; membership is
    // set-based, so loader timing cannot affect these assertions. The query
    // center follows the drop down so the view radius never clips it.
    let (_save, mut reference, mut tick, position) = setup("drop-crash-ref");
    let mut trajectory = Vec::new();
    let mut center = position;
    for _ in 0..2_000 {
        tick_once(&mut reference, TickId::new(tick), Instant::now()).unwrap();
        tick += 1;
        let nearby = drop_nearby(&reference, center);
        assert_eq!(nearby.len(), 1);
        center = nearby[0].position;
        trajectory.push(center);
        if drop_active_len(&reference) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&reference), 0);
    let rest = trajectory.last().copied().unwrap();

    let (save, mut state, mut tick, _) = setup("drop-motion-crash-whole");
    for _ in 0..4 {
        tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
        tick += 1;
    }
    assert!(trajectory.contains(&drop_nearby(&state, position)[0].position));

    // Crash with the next motion step staged but unapplied: recovery lands
    // on a whole record from the trajectory — the last complete one when
    // the receipt never synced, the replayed one when it did — never a
    // half-moved drop in between.
    let id = crate::server::entities::EntityId::new(drop_nearby(&state, position)[0].id).unwrap();
    let mut staged_tick = None;
    for candidate in tick..tick + 8 {
        if let Some(action) = plan_entity_tick(&mut state, id, candidate, false).unwrap() {
            let permit = state
                .durability
                .entity_mirror
                .try_reserve_durable()
                .unwrap()
                .expect("mirror admits the crash-probe motion");
            assert!(
                state
                    .durability
                    .try_stage(TickId::new(candidate), &action, Some(permit))
                    .unwrap()
            );
            staged_tick = Some(candidate);
            break;
        }
    }
    let staged_tick = staged_tick.expect("a falling drop has due motion");
    drop(state);
    let mut reopened = state_for(&save, 7);
    let recovered = drop_nearby(&reopened, position)[0].position;
    assert!(
        trajectory.contains(&recovered),
        "crash recovery must land on a whole record, got {recovered:?}"
    );
    // The crash changes nothing eventual: the reopened drop keeps falling
    // through whole records and rests exactly where the reference did,
    // with every item conserved.
    let mut center = position;
    for candidate in staged_tick..staged_tick + 2_000 {
        tick_once(&mut reopened, TickId::new(candidate), Instant::now()).unwrap();
        let nearby = drop_nearby(&reopened, center);
        assert_eq!(nearby.len(), 1);
        center = nearby[0].position;
        assert!(
            trajectory.contains(&center),
            "reopened motion left the trajectory at {center:?}"
        );
        if drop_active_len(&reopened) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&reopened), 0);
    let landed = drop_nearby(&reopened, center);
    assert_eq!(landed.len(), 1);
    assert_eq!(landed[0].count, 7, "crash recovery conserves every item");
    assert_eq!(landed[0].position, rest);
}

#[test]
fn drops_conserve_and_cap_across_spawn_fall_merge_take_expiry_restart() {
    let save = TestSave::new("drop-full-lifecycle");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    let top = [0.5, surface_y as f32 + 64.0, 0.5];
    let mut tick = 1;
    // Spawn splits over the 128 cap; the fall crosses chunk owners.
    spawn_drop(&mut state, tick, top, STONE_ITEM, 300, Duration::ZERO);
    let mut counts: Vec<u16> = drop_nearby(&state, top)
        .iter()
        .map(|drop| drop.count)
        .collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![44, 128, 128]);
    for _ in 0..6_000 {
        run_empty_tick(&mut state, &mut tick);
        if drop_active_len(&state) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&state), 0);
    let settled = drop_nearby(&state, [0.5, surface_y as f32, 0.5]);
    assert_eq!(
        settled
            .iter()
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        300
    );
    // A staggered spawn merges into the settled remainder stack.
    let partial = settled
        .iter()
        .find(|drop| drop.count == 44)
        .unwrap()
        .position;
    spawn_drop(&mut state, tick, partial, STONE_ITEM, 50, Duration::ZERO);
    let mut merged: Vec<u16> = drop_nearby(&state, [0.5, surface_y as f32, 0.5])
        .iter()
        .map(|drop| drop.count)
        .collect();
    merged.sort_unstable();
    assert_eq!(merged, vec![94, 128, 128]);
    // Take from a full stack, then restart: totals and caps hold.
    let victim = crate::server::entities::EntityId::new(
        settled.iter().find(|drop| drop.count == 128).unwrap().id,
    )
    .unwrap();
    take_drop(&mut state, tick, victim, 100);
    assert_eq!(
        drop_nearby(&state, [0.5, surface_y as f32, 0.5])
            .iter()
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        250
    );
    assert_eq!(
        drop_stack(&state, victim).map(|stack| stack.count),
        Some(28)
    );
    drop(state);
    let mut restarted = state_for(&save, 7);
    let mut reopened: Vec<u16> = drop_nearby(&restarted, [0.5, surface_y as f32, 0.5])
        .iter()
        .map(|drop| drop.count)
        .collect();
    reopened.sort_unstable();
    assert_eq!(reopened, vec![28, 94, 128]);
    // Expiry removes every drop past its lifetime in one bounded record.
    let far_future = crate::server::drops::unix_ms() + 601_000;
    let expired = crate::server::drops::plan_expired(
        &restarted.entities,
        restarted.world.catalog(),
        far_future,
        256,
    )
    .unwrap()
    .expect("aged drops plan expiry");
    let permit = restarted
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .expect("mirror admits the expiry batch");
    let action = crate::server::durable::CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        sounds: Vec::new(),
        player_publication: None,
        entities: Some(expired),
    };
    assert!(
        restarted
            .durability
            .try_stage(TickId::new(tick), &action, Some(permit))
            .unwrap()
    );
    drain_durable(&mut restarted, tick);
    assert!(drop_nearby(&restarted, [0.5, surface_y as f32, 0.5]).is_empty());
    drop(restarted);
    let empty = state_for(&save, 7);
    assert!(drop_nearby(&empty, [0.5, surface_y as f32, 0.5]).is_empty());
}
