//! Step-one coordinator pins for world drops, now on the shared entity path.
//!
//! Observable behaviour before and after the migration onto the shared
//! entity path. Ported mechanically from the `state.drops` API onto
//! WAL-staged entity helpers: only call syntax changed, every behavioural
//! assertion is unmodified. Explicit `save()` calls were deleted rather
//! than renamed: on the unified path every staged receipt is already
//! journal-durable, so there is no separate drop file left to flush.

use super::*;
use crate::server::entities::EntityId;

#[test]
fn listener_drop_motion_merge_pickup_and_restart_use_one_durable_path() {
    use std::net::{TcpListener, TcpStream};
    let save = TestSave::new(&format!(
        "drop-listener-motion-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let profile = 0x3434;
    let mut state = state_for(&save, 7);
    // Exercise the returning-player position path as well as WAL recovery.
    state
        .position_store
        .save(profile, state.spawn_anchor)
        .unwrap();
    let start = [0.5, state.spawn_anchor[1] + 7.0, 0.5];
    spawn_drop(&mut state, 1, start, PIN_STONE, 80, Duration::ZERO);
    spawn_drop(&mut state, 1, start, PIN_STONE, 80, Duration::ZERO);
    assert_eq!(drop_totals(&state, start), (2, 160));
    let catalog = state.world.catalog_arc();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = std::sync::mpsc::channel();
    let server = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            crate::server::net::serve_listener_with_stats(
                listener,
                Box::new(state),
                stopped,
                Arc::new(crate::server::net::TransportStats::default()),
            )
        })
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "drop-motion".into(),
                profile,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        loop {
            let ServerMessage::ContentManifestPart {
                fingerprint,
                total_len,
                offset,
                bytes,
            } = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap()
            else {
                panic!("expected manifest");
            };
            assert_eq!(fingerprint, catalog.fingerprint());
            if offset as usize + bytes.len() == total_len as usize {
                break;
            }
        }
        protocol::write_client(
            &mut peer,
            &ClientMessage::ContentReady {
                fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut positions = std::collections::BTreeSet::new();
        let mut saw_merge = false;
        let mut saw_pickup = false;
        let mut credited = false;
        while Instant::now() < deadline
            && !(saw_merge && positions.len() > 1 && saw_pickup && credited)
        {
            match protocol::read_server_with_catalog(&mut peer, &catalog) {
                Ok(ServerMessage::Drops { items, .. }) => {
                    if items.iter().map(|item| u32::from(item.count)).sum::<u32>() == 160 {
                        saw_merge |= items.len() == 2
                            && items.iter().any(|item| item.count == 128)
                            && items.iter().any(|item| item.count == 32);
                        if let Some(item) = items.first() {
                            positions.insert(item.position[1].to_bits());
                        }
                    }
                }
                Ok(ServerMessage::Pickups { items }) => {
                    saw_pickup |= items.iter().any(|item| item.item == PIN_STONE);
                }
                Ok(ServerMessage::Inventory { slots, .. }) => {
                    credited |= slots
                        .iter()
                        .flatten()
                        .map(|stack| {
                            if stack.item == PIN_STONE {
                                u32::from(stack.count)
                            } else {
                                0
                            }
                        })
                        .sum::<u32>()
                        == 160;
                }
                Ok(_) => {}
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(error) => panic!("listener read failed: {error}"),
            }
        }
        assert!(saw_merge, "listener did not publish capped merge");
        assert!(
            positions.len() > 1,
            "listener did not publish falling motion"
        );
        assert!(saw_pickup, "listener did not send explicit pickup");
        assert!(credited, "listener did not credit finite inventory");
        peer
    }));
    stop.send(()).unwrap();
    server.join().unwrap().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    let restarted = state_for(&save, 7);
    assert_eq!(drop_totals(&restarted, start).1, 0);
    let inventory = restarted.inventory_store.load(profile).unwrap();
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.item == PIN_STONE)
            .map(|stack| u32::from(stack.count))
            .sum::<u32>(),
        160
    );
}

const PIN_STONE: crate::items::ItemId = crate::items::ItemId::new(crate::world::STONE.get());

fn pin_action_id(state: &State, profile: u128, seq: u64) -> u128 {
    (u128::from(state.durability.receipt_ledger(profile).current_epoch()) << 64) | u128::from(seq)
}

fn drop_totals(state: &State, at: [f32; 3]) -> (usize, u32) {
    let nearby = drop_nearby(state, at);
    (
        nearby.len(),
        nearby.iter().map(|drop| u32::from(drop.count)).sum(),
    )
}

#[test]
fn pin_spawn_merge_and_cap_hold_at_coordinator_level() {
    let save = TestSave::new("drop-pin-merge");
    let mut state = state_for(&save, 7);
    spawn_drop(
        &mut state,
        1,
        [4.0, 4.0, 4.0],
        PIN_STONE,
        300,
        Duration::ZERO,
    );
    let nearby = drop_nearby(&state, [4.0, 4.0, 4.0]);
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
    reside_neighbourhood(&mut empty, position);
    reside_neighbourhood(&mut busy, position);
    spawn_drop(&mut empty, 1, position, PIN_STONE, 1, Duration::ZERO);
    spawn_drop(&mut busy, busy_tick, position, PIN_STONE, 1, Duration::ZERO);
    for step in 0..12 {
        tick_once(&mut empty, TickId::new(1 + step), Instant::now()).unwrap();
        tick_once(&mut busy, TickId::new(busy_tick + step), Instant::now()).unwrap();
        // Receipt synchronization, matching the simulation trajectory test:
        // both states apply every staged step before the comparison.
        drain_durable(&mut empty, 1 + step);
        drain_durable(&mut busy, busy_tick + step);
        let alone = drop_nearby(&empty, position);
        let crowded = drop_nearby(&busy, position);
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
    let mut tick = 1;
    spawn_drop(
        &mut state,
        tick,
        [15.9, surface_y as f32 + 4.0, 0.5],
        PIN_STONE,
        2,
        Duration::ZERO,
    );
    spawn_drop(
        &mut state,
        tick,
        [16.9, surface_y as f32 + 4.0, 0.5],
        PIN_STONE,
        3,
        Duration::ZERO,
    );
    for _ in 0..2_000 {
        run_empty_tick(&mut state, &mut tick);
        if drop_active_len(&state) == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drop_active_len(&state), 0);
    // Both drops settled with every item conserved. (Local terrain varies,
    // so a drop that starts inside a hill pops up above its spawn height.)
    let settled = drop_nearby(&state, [16.4, surface_y as f32, 0.5]);
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
        drop_nearby(&state, [16.4, surface_y as f32, 0.5])
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
    let mut tick = 1;
    spawn_drop(&mut state, tick, fresh, PIN_STONE, 1, Duration::ZERO);
    spawn_drop(
        &mut state,
        tick,
        delayed,
        PIN_STONE,
        1,
        Duration::from_secs(3_600),
    );
    let first = drop_nearby(&state, fresh)[0].age_ms;
    std::thread::sleep(Duration::from_millis(5));
    run_empty_tick(&mut state, &mut tick);
    let second = drop_nearby(&state, fresh)
        .into_iter()
        .find(|drop| drop.position[0] < 3.0)
        .unwrap()
        .age_ms;
    assert!(second >= first, "server drop age must never run backwards");
    // Only the delay-free drop is pickable; the delayed one stays visible.
    let fresh_candidates = drop_candidates(&state, fresh);
    assert_eq!(fresh_candidates.len(), 1);
    assert!(drop_candidates(&state, delayed).is_empty());
    let visible = drop_nearby(&state, delayed);
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
    spawn_drop(&mut state, tick, at, PIN_STONE, 1, Duration::ZERO);
    // No Move, no animation handshake: proximity alone drives the pickup.
    wait_for_inventory(&mut state, &mut tick, session.id, |inventory| {
        inventory.slots.iter().any(|slot| {
            slot.as_ref()
                .is_some_and(|stack| stack.item == PIN_STONE && stack.count == 1)
        })
    });
    assert!(drop_nearby(&state, at).is_empty());
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
    spawn_drop(
        &mut state,
        1,
        [0.5, surface_y as f32 + 0.25, 0.5],
        PIN_STONE,
        5,
        Duration::ZERO,
    );
    spawn_drop(
        &mut state,
        1,
        [40.5, surface_y as f32 + 0.25, 0.5],
        PIN_STONE,
        130,
        Duration::ZERO,
    );
    let before = drop_nearby(&state, [20.0, surface_y as f32, 0.5]);
    assert_eq!(before.len(), 3);
    drop(state);

    let restarted = state_for(&save, 7);
    let after = drop_nearby(&restarted, [20.0, surface_y as f32, 0.5]);
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
    spawn_drop(&mut state, 1, at, PIN_STONE, 200, Duration::ZERO);
    assert_eq!(drop_totals(&state, at).1, 200);

    let victim = EntityId::new(drop_nearby(&state, at)[0].id).unwrap();
    let before = drop_stack(&state, victim).unwrap().count;
    take_drop(&mut state, 1, victim, 50);
    assert_eq!(drop_totals(&state, at).1, 150);
    assert_eq!(
        drop_stack(&state, victim).map(|stack| stack.count),
        Some(before - 50)
    );

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
    save_inventory(&save, 7, profile, &inventory);
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
