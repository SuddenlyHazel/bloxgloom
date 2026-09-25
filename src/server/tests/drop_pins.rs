//! Step-one coordinator pins for world drops.
//!
//! Observable behaviour before the migration onto the shared entity path.
//! These must pass UNMODIFIED after the migration: a pin that has to change
//! is a behaviour change.

use super::*;

const PIN_STONE: crate::items::ItemId = crate::items::ItemId::new(crate::world::STONE.get());

fn pin_action_id(state: &State, profile: u128, seq: u64) -> u128 {
    (u128::from(state.durability.receipt_ledger(profile).current_epoch()) << 64) | u128::from(seq)
}

fn drop_totals(state: &State, at: [f32; 3]) -> (usize, u32) {
    let nearby = state.drops.nearby(at);
    (
        nearby.len(),
        nearby.iter().map(|drop| u32::from(drop.count)).sum(),
    )
}

#[test]
fn pin_spawn_merge_and_cap_hold_at_coordinator_level() {
    let save = TestSave::new("drop-pin-merge");
    let mut state = state_for(&save, 7);
    state
        .drops
        .spawn([4.0, 4.0, 4.0], PIN_STONE, 300, Duration::ZERO);
    let nearby = state.drops.nearby([4.0, 4.0, 4.0]);
    let mut counts: Vec<u16> = nearby.iter().map(|drop| drop.count).collect();
    counts.sort_unstable();
    assert_eq!(counts, vec![44, 128, 128]);
    assert!(counts.iter().all(|count| *count <= 128));
}

#[test]
fn pin_trajectory_matches_across_player_counts_and_ticks() {
    let empty_save = TestSave::new("drop-pin-no-clients");
    let busy_save = TestSave::new("drop-pin-clients");
    let mut empty = state_for(&empty_save, 7);
    let mut busy = state_for(&busy_save, 7);
    let mut busy_tick = 1;
    let sessions: Vec<_> = (1..=4)
        .map(|profile| join(&mut busy, &mut busy_tick, profile))
        .collect();
    assert_eq!(busy.clients.len(), 4);

    let position = [0.5, crate::world::MAX_GENERATED_HEIGHT as f32 + 20.0, 0.5];
    for state in [&mut empty, &mut busy] {
        state.drops.spawn(position, PIN_STONE, 1, Duration::ZERO);
    }
    for step in 0..12 {
        tick_once(&mut empty, TickId::new(1 + step), Instant::now()).unwrap();
        tick_once(&mut busy, TickId::new(busy_tick + step), Instant::now()).unwrap();
        let alone = empty.drops.nearby(position);
        let crowded = busy.drops.nearby(position);
        assert_eq!(alone.len(), 1);
        assert_eq!(crowded.len(), 1);
        assert_eq!(
            (
                alone[0].id,
                alone[0].item,
                alone[0].count,
                alone[0].position
            ),
            (
                crowded[0].id,
                crowded[0].item,
                crowded[0].count,
                crowded[0].position
            ),
            "player count changed drop state at step {step}"
        );
    }
    drop(sessions);
}

#[test]
fn pin_drops_settle_across_chunk_seam_without_loss() {
    let save = TestSave::new("drop-pin-seam");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    // 15.9 straddles the x=16 chunk boundary in its physics samples; 16.9
    // sits a full block away so the two spawns never merge. The settle loop
    // sleeps so the async chunk loader can deliver missing terrain.
    state.drops.spawn(
        [15.9, surface_y as f32 + 4.0, 0.5],
        PIN_STONE,
        2,
        Duration::ZERO,
    );
    state.drops.spawn(
        [16.9, surface_y as f32 + 4.0, 0.5],
        PIN_STONE,
        3,
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
    // Both drops settled with every item conserved. (Local terrain varies,
    // so a drop that starts inside a hill pops up above its spawn height.)
    let settled = state.drops.nearby([16.4, surface_y as f32, 0.5]);
    assert_eq!(settled.len(), 2);
    for drop in &settled {
        assert!(drop.position[1].is_finite());
    }
    assert_eq!(
        settled
            .iter()
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        5
    );
    // Settled drops stay put (positions, not ages: the server clock advances).
    let positions: Vec<(u64, [f32; 3])> = settled
        .iter()
        .map(|drop| (drop.id, drop.position))
        .collect();
    for _ in 0..8 {
        run_empty_tick(&mut state, &mut tick);
    }
    assert_eq!(
        state
            .drops
            .nearby([16.4, surface_y as f32, 0.5])
            .iter()
            .map(|drop| (drop.id, drop.position))
            .collect::<Vec<_>>(),
        positions
    );
}

#[test]
fn pin_server_age_is_monotonic_and_owns_pickup_eligibility() {
    let save = TestSave::new("drop-pin-age");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    let fresh = [0.5, surface_y as f32 + 0.25, 0.5];
    let delayed = [5.5, surface_y as f32 + 0.25, 0.5];
    state.drops.spawn(fresh, PIN_STONE, 1, Duration::ZERO);
    state
        .drops
        .spawn(delayed, PIN_STONE, 1, Duration::from_secs(3_600));
    let mut tick = 1;
    let first = state.drops.nearby(fresh)[0].age_ms;
    std::thread::sleep(Duration::from_millis(5));
    run_empty_tick(&mut state, &mut tick);
    let second = state
        .drops
        .nearby(fresh)
        .into_iter()
        .find(|drop| drop.position[0] < 3.0)
        .unwrap()
        .age_ms;
    assert!(second >= first, "server drop age must never run backwards");
    // Only the delay-free drop is pickable; the delayed one stays visible.
    let fresh_candidates = state.drops.pickup_candidates(fresh);
    assert_eq!(fresh_candidates.len(), 1);
    assert!(state.drops.pickup_candidates(delayed).is_empty());
    let visible = state.drops.nearby(delayed);
    assert_eq!(visible.len(), 2);
    // Physics only moves drops vertically, so the delayed drop keeps its x.
    assert!(visible.iter().any(|drop| drop.position[0] == 5.5));
}

#[test]
fn pin_live_pickup_needs_no_client_timing_signal() {
    let save = TestSave::new("drop-pin-live-pickup");
    let profile = 4242;
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let at = session.joined.position;
    state.drops.spawn(at, PIN_STONE, 1, Duration::ZERO);
    // No Move, no animation handshake: proximity alone drives the pickup.
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots.iter().any(|slot| {
            slot.as_ref()
                .is_some_and(|stack| stack.item == PIN_STONE && stack.count == 1)
        })
    });
    assert!(state.drops.nearby(at).is_empty());
    assert!(
        messages(&session)
            .iter()
            .any(|message| matches!(message, ServerMessage::Pickups { .. }))
    );
    drop(session);
}

#[test]
fn pin_restart_preserves_drops_identically() {
    let save = TestSave::new("drop-pin-restart");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    state.drops.spawn(
        [0.5, surface_y as f32 + 0.25, 0.5],
        PIN_STONE,
        5,
        Duration::ZERO,
    );
    state.drops.spawn(
        [40.5, surface_y as f32 + 0.25, 0.5],
        PIN_STONE,
        130,
        Duration::ZERO,
    );
    let before = state.drops.nearby([20.0, surface_y as f32, 0.5]);
    assert_eq!(before.len(), 3);
    state.drops.save().unwrap();
    drop(state);

    let restarted = state_for(&save, 7);
    let after = restarted.drops.nearby([20.0, surface_y as f32, 0.5]);
    assert_eq!(
        after
            .iter()
            .map(|drop| (drop.id, drop.item, drop.count, drop.position))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|drop| (drop.id, drop.item, drop.count, drop.position))
            .collect::<Vec<_>>()
    );
}

#[test]
fn pin_conservation_across_spawn_take_and_restart() {
    let save = TestSave::new("drop-pin-conservation");
    let mut state = state_for(&save, 7);
    let surface_y = state.spawn_anchor[1] as i32;
    let at = [0.5, surface_y as f32 + 0.25, 0.5];
    state.drops.spawn(at, PIN_STONE, 200, Duration::ZERO);
    assert_eq!(drop_totals(&state, at).1, 200);

    let victim = state.drops.nearby(at)[0].id;
    let before = state.drops.stack(victim).unwrap().count;
    state.drops.take(victim, 50);
    assert_eq!(drop_totals(&state, at).1, 150);
    assert_eq!(
        state.drops.stack(victim).map(|stack| stack.count),
        Some(before - 50)
    );

    state.drops.save().unwrap();
    drop(state);
    let restarted = state_for(&save, 7);
    assert_eq!(drop_totals(&restarted, at).1, 150);
}

#[test]
fn pin_thrown_drop_pickup_survives_restart_with_inventory() {
    let save = TestSave::new("drop-pin-throw-restart");
    let profile = 777;
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(PIN_STONE, 1));
    save_inventory(&save, profile, &inventory);
    let mut state = state_for(&save, 7);
    let mut tick = 1;
    let session = join(&mut state, &mut tick, profile);
    let drop_id = pin_action_id(&state, profile, 1);
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
    // The thrown item still exists exactly once: not in inventory, one drop.
    let (_, total) = drop_totals(&state, session.joined.position);
    assert_eq!(total, 1);
    drop(session);
}
