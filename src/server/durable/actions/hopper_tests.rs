use super::*;
use crate::content::{HOPPER_ITEM, HOPPER_STATE, KILN_DEFAULT_STATE, KILN_ITEM};
use crate::inventory::Stack;
use crate::server::entities::machine::MachinePayload as HopperPayload;

fn pulse(state: &mut State, tick: &mut u64, count: u64) {
    for _ in 0..count {
        *tick += 1;
        crate::server::runtime::tick_once(state, TickId::new(*tick), Instant::now()).unwrap();
    }
}

fn hopper(state: &State, y: i32) -> HopperPayload {
    let id = state
        .entities
        .anchored_at(CellCoord::new(-1, y, -1))
        .unwrap();
    state
        .entities
        .snapshot(id)
        .unwrap()
        .private_payload
        .downcast_ref::<HopperPayload>()
        .unwrap()
        .clone()
}

#[test]
fn hopper_feeds_kiln_collects_output_and_survives_restart_then_break() {
    let path = temp_save_dir("hopper-production-chain");
    let mut state = server_state(53, path.clone()).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(HOPPER_ITEM, 2));
    inventory.slots[1] = Some(Stack::new(KILN_ITEM, 1));
    inventory.slots[25] = Some(Stack::new(ItemId(crate::world::GRAVEL.0), 3));
    inventory.slots[26] = Some(Stack::new(STICK, 2));
    let peer = add_test_client(&mut state, [-1.0, 80.0, 2.0], inventory);
    for x in -2..=0 {
        for y in 3..=6 {
            for z in -2..=0 {
                state
                    .world
                    .get_chunk(crate::world::ChunkKey { x, y, z })
                    .unwrap();
            }
        }
    }
    let epoch = grant_action_epoch(&mut state, 17);
    let mut seq = 0;
    let mut next = || {
        seq += 1;
        (u128::from(epoch) << 64) | seq
    };
    let mut tick = 40_000;
    for (y, block, slot) in [
        (79, HOPPER_STATE, 0),
        (80, KILN_DEFAULT_STATE, 1),
        (82, HOPPER_STATE, 0),
    ] {
        settle_live_action(
            &mut state,
            tick,
            ClientMessage::Edit {
                action_id: next(),
                x: -1,
                y,
                z: -1,
                block,
                slot,
            },
        );
        tick += 1;
        assert!(
            state
                .entities
                .anchored_at(CellCoord::new(-1, y, -1))
                .is_some()
        );
    }
    for (slot, player_slot, count) in [(0, 25, 3u16), (1, 26, 2u16)] {
        let id = state
            .entities
            .anchored_at(CellCoord::new(-1, 82, -1))
            .unwrap();
        let snapshot = state.entities.snapshot(id).unwrap();
        let mut payload = vec![2, 0, slot, player_slot];
        payload.extend(count.to_le_bytes());
        payload.extend(id.get().to_le_bytes());
        payload.extend(snapshot.revision.to_le_bytes());
        settle_live_action(
            &mut state,
            tick,
            ClientMessage::EntityInteract {
                action_id: next(),
                target: [-1, 82, -1],
                payload,
            },
        );
        tick += 1;
    }
    pulse(&mut state, &mut tick, 100);
    // Restart in the middle of the production chain, preserving its late clock.
    drop(peer);
    drop(state);
    let mut state = server_state(53, path.clone()).unwrap();
    tick = state.recovered_tick;
    for x in -2..=0 {
        for y in 3..=6 {
            for z in -2..=0 {
                state
                    .world
                    .get_chunk(crate::world::ChunkKey { x, y, z })
                    .unwrap();
            }
        }
    }
    // Transfers use async receipts; under concurrent test I/O a fixed 400
    // attempted ticks is not a fixed number of admitted processing steps.
    for _ in 0..40 {
        pulse(&mut state, &mut tick, 20);
        if hopper(&state, 79)
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == ItemId(crate::world::STONE.0))
            .map(|s| s.count)
            .sum::<u16>()
            == 3
        {
            break;
        }
    }
    let output = hopper(&state, 79);
    assert_eq!(
        output
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == ItemId(crate::world::STONE.0))
            .map(|s| s.count)
            .sum::<u16>(),
        3
    );
    assert!(hopper(&state, 82).slots.iter().all(Option::is_none));
    let inventory = state.inventory_store.load(17).unwrap();
    let peer = add_test_client(&mut state, [-1.0, 80.0, 2.0], inventory);
    let epoch = grant_action_epoch(&mut state, 17);
    settle_live_action(
        &mut state,
        tick + 1,
        ClientMessage::Edit {
            action_id: (u128::from(epoch) << 64) | 1,
            x: -1,
            y: 79,
            z: -1,
            block: AIR,
            slot: 0,
        },
    );
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .is_none()
    );
    let drops = crate::server::drops::nearby(&state.entities, [-0.5, 79.5, -0.5]);
    let stacks: Vec<_> = drops
        .iter()
        .map(|d| {
            crate::server::drops::stack(
                &state.entities,
                crate::server::entities::EntityId::new(d.id).unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert!(stacks.iter().any(|s| s.item == HOPPER_ITEM && s.count == 1));
    assert!(
        stacks
            .iter()
            .any(|s| s.item == ItemId(crate::world::STONE.0) && s.count == 3)
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn hopper_push_is_atomic_conflict_checked_and_resumes_after_full_destination() {
    use crate::server::entities::{EntityPatch, EntityPayload};
    let path = temp_save_dir("hopper-push-conflict");
    let mut state = server_state(53, path.clone()).unwrap();
    for x in -1..=1 {
        for y in 4..=6 {
            for z in -1..=1 {
                state
                    .world
                    .get_chunk(crate::world::ChunkKey { x, y, z })
                    .unwrap();
            }
        }
    }
    let item = ItemId(crate::world::STONE.0);
    let mut ids = Vec::new();
    for (y, slots) in [
        (81, [Some(Stack::new(item, 2)), None, None]),
        (80, std::array::from_fn(|_| Some(Stack::new(item, 128)))),
    ] {
        state.world.edit(0, y, 0, HOPPER_STATE).unwrap();
        let transaction = state
            .entities
            .prepare_spawn(crate::server::entities::EntitySpawn::Anchored {
                entity_type: crate::content::HOPPER_ENTITY_TYPE,
                anchor: CellCoord::new(0, y, 0),
                anchor_state: HOPPER_STATE,
                footprint: vec![CellCoord::new(0, y, 0)],
                payload: EntityPayload::new(HopperPayload {
                    slots: slots.to_vec(),
                    ..HopperPayload::empty(3, 0)
                }),
                spawn_tick: 0,
            })
            .unwrap();
        ids.push(transaction.entity_id());
        state.entities.apply_committed(transaction).unwrap();
    }
    let [source, destination] = [ids[0], ids[1]];
    let plan = |state: &mut State| {
        plan_durable_request(
            state,
            &DurableRequest::EntityTick { id: source },
            TickId::new(21),
        )
        .unwrap()
        .unwrap()
    };
    let blocked = plan(&mut state);
    assert_eq!(
        blocked.entities.as_ref().unwrap().entity_ids(),
        vec![source],
        "full output only advances the hopper's own schedule"
    );
    let set_destination = |state: &mut State, count| {
        let snapshot = state.entities.snapshot(destination).unwrap();
        let mut payload = snapshot
            .private_payload
            .downcast_ref::<HopperPayload>()
            .unwrap()
            .clone();
        payload.slots[0].as_mut().unwrap().count = count;
        let update = state
            .entities
            .prepare_update(
                destination,
                snapshot.revision,
                EntityPatch {
                    payload: Some(EntityPayload::new(payload)),
                    next_tick: None,
                    position: None,
                },
            )
            .unwrap();
        state.entities.apply_committed(update).unwrap();
    };
    set_destination(&mut state, 127);
    let planned = plan(&mut state);
    assert_eq!(planned.entities.as_ref().unwrap().entity_ids().len(), 2);
    assert_eq!(
        state
            .entities
            .snapshot(source)
            .unwrap()
            .private_payload
            .downcast_ref::<HopperPayload>()
            .unwrap()
            .slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    set_destination(&mut state, 128);
    assert!(
        state
            .entities
            .validate_prepared(planned.entities.as_ref().unwrap())
            .is_err(),
        "concurrent fill invalidates the whole push"
    );
    set_destination(&mut state, 127);
    let next_due = state.entities.snapshot(destination).unwrap().next_tick;
    let planned = plan(&mut state);
    state
        .entities
        .apply_committed(planned.entities.unwrap())
        .unwrap();
    assert_eq!(
        state.entities.snapshot(destination).unwrap().next_tick,
        next_due,
        "push preserves recipient scheduling"
    );
    assert_eq!(state.entities.snapshot(source).unwrap().next_tick, Some(41));
    assert_eq!(
        state
            .entities
            .snapshot(source)
            .unwrap()
            .private_payload
            .downcast_ref::<HopperPayload>()
            .unwrap()
            .slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        state
            .entities
            .snapshot(destination)
            .unwrap()
            .private_payload
            .downcast_ref::<HopperPayload>()
            .unwrap()
            .slots[0]
            .as_ref()
            .unwrap()
            .count,
        128
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
