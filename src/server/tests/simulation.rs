use super::*;

#[test]
fn zero_client_tick_advances_world_drops() {
    let save = TestSave::new("unattended-tick");
    let mut state = state_for(&save, 7);
    assert!(state.clients.is_empty());
    let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    let stone = crate::items::ItemId::new(crate::world::STONE.get());
    reside_neighbourhood(&mut state, position);
    spawn_drop(&mut state, 1, position, stone, 1, Duration::ZERO);
    let before = drop_nearby(&state, position)[0].position[1];

    // Motion stages through the WAL: ticks queue and plan the step while a
    // later receipt applies it, so the test polls until the fall lands
    // instead of assuming a single-tick step.
    let mut moved = false;
    for tick in 1..501 {
        tick_once(&mut state, TickId::new(tick), Instant::now()).unwrap();
        if drop_nearby(&state, position)[0].position[1] < before {
            moved = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    let after = drop_nearby(&state, position)[0].position[1];
    assert!(
        moved && after < before,
        "drop did not advance during an unattended tick"
    );
}

#[test]
fn landed_drop_checkpoint_drains_and_does_not_stall_rotation() {
    let save = TestSave::new("landed-drop-rotation");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let surface_y = state.spawn_anchor[1] as i32;
    let stone = crate::items::ItemId::new(crate::world::STONE.get());
    spawn_drop(
        &mut state,
        tick,
        [0.5, surface_y as f32 + 0.25, 0.5],
        stone,
        1,
        Duration::ZERO,
    );

    for _ in 0..200 {
        run_empty_tick(&mut state, &mut tick);
        if drop_active_len(&state) == 0
            && state.durability.pending.is_empty()
            && state.durability.dirty_checkpoints.is_empty()
            && state.durability.checkpoint_inflight.is_empty()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&state), 0);
    assert!(state.durability.pending.is_empty());
    assert!(state.durability.dirty_checkpoints.is_empty());
    assert!(state.durability.checkpoint_inflight.is_empty());

    let completed_rotations = state.durability.completed_rotations;
    state.durability.rotation_requested = true;
    run_empty_tick(&mut state, &mut tick);
    if state.durability.rotation_requested {
        // No accepted transactions or dirty files remain. The first tick
        // must therefore enqueue the entity fence, not defer on other work.
        // Await the actual fsync receipt instead of running an arbitrary
        // number of ticks while a filesystem worker races the test thread.
        state
            .durability
            .entity_checkpoint_ticket
            .as_mut()
            .expect("drained landed-drop state must enqueue its entity checkpoint")
            .wait_for_worker(Duration::from_secs(5))
            .unwrap_or_else(|error| {
                panic!(
                    "{error}; mirror={:?}",
                    state.durability.entity_mirror.metrics()
                )
            });
        run_empty_tick(&mut state, &mut tick);
        if state.durability.rotation_requested {
            let receiver = state
                .durability
                .rotation_receipt
                .take()
                .expect("completed checkpoint coverage must enqueue WAL rotation");
            let receipt = receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("WAL generation switch must return its completion receipt");
            // Relay the real result back to the normal production poll, which
            // checks its cut sequence, releases the fence, and clears rotation.
            let (sender, receiver) = mpsc::channel();
            sender.send(receipt).unwrap();
            state.durability.rotation_receipt = Some(receiver);
            run_empty_tick(&mut state, &mut tick);
        }
    }
    assert!(!state.durability.rotation_requested);
    assert!(state.durability.rotation_receipt.is_none());
    assert!(state.durability.entity_checkpoint_ticket.is_none());
    assert!(!state.durability.entity_mirror.metrics().fenced);
    assert_eq!(
        state.durability.completed_rotations,
        completed_rotations + 1
    );

    let session = join(&mut state, &mut tick, 3003);
    let output = command_and_wait(
        &mut state,
        &mut tick,
        &session,
        1,
        session.action_id(1),
        ClientMessage::Edit {
            action_id: session.action_id(1),
            x: 0,
            y: surface_y - 1,
            z: 0,
            block: AIR,
            slot: 0,
        },
    );
    assert_eq!(action_result(&output, session.action_id(1)), Some(true));
}

#[test]
fn player_count_does_not_change_authoritative_drop_trajectory() {
    let empty_save = TestSave::new("drop-no-clients");
    let populated_save = TestSave::new("drop-sixteen-clients");
    let mut empty = state_for(&empty_save, 7);
    let mut populated = state_for(&populated_save, 7);
    let mut populated_tick = 1;
    let sessions: Vec<_> = (1..=16)
        .map(|profile| join(&mut populated, &mut populated_tick, profile))
        .collect();
    assert_eq!(populated.clients.len(), 16);

    let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    let stone = crate::items::ItemId::new(crate::world::STONE.get());
    reside_neighbourhood(&mut empty, position);
    reside_neighbourhood(&mut populated, position);
    spawn_drop(&mut empty, 1, position, stone, 1, Duration::ZERO);
    spawn_drop(
        &mut populated,
        populated_tick,
        position,
        stone,
        1,
        Duration::ZERO,
    );
    for offset in 0..8 {
        tick_once(&mut empty, TickId::new(1 + offset), Instant::now()).unwrap();
        tick_once(
            &mut populated,
            TickId::new(populated_tick + offset),
            Instant::now(),
        )
        .unwrap();
        // Receipt synchronization: both states apply every staged step
        // before the comparison, so the per-step snapshots prove the physics
        // is load-independent rather than receipt-timing-dependent.
        drain_durable(&mut empty, 1 + offset);
        drain_durable(&mut populated, populated_tick + offset);
        let empty_drop = drop_nearby(&empty, position);
        let populated_drop = drop_nearby(&populated, position);
        assert_eq!(empty_drop.len(), 1);
        assert_eq!(populated_drop.len(), 1);
        assert_eq!(
            (
                empty_drop[0].id,
                empty_drop[0].item,
                empty_drop[0].count,
                empty_drop[0].position,
            ),
            (
                populated_drop[0].id,
                populated_drop[0].item,
                populated_drop[0].count,
                populated_drop[0].position,
            ),
            "player count changed drop state at step {offset}"
        );
    }
    drop(sessions);
}

#[test]
fn coordinator_records_real_movement_worker_capacity() {
    let save = TestSave::new("movement-worker-telemetry");
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, 501);

    run_tick(
        &mut state,
        &mut tick,
        vec![SimulationInput::Command {
            id: session.id,
            sequence: 1,
            message: ClientMessage::Move {
                seq: 1,
                dx: 0.1,
                dy: 0.0,
                dz: 0.0,
            },
        }],
    );

    let sample = state.metrics.latest().unwrap();
    assert!(sample.movement_worker_capacity_nanos > 0);
    assert!(sample.movement_worker_busy_nanos <= sample.movement_worker_capacity_nanos);
    assert!(
        state
            .metrics
            .movement_worker_utilization_percent()
            .is_some()
    );
}
