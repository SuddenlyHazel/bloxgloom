use super::*;
use crate::content::{CHEST_ITEM, CHEST_STATE, HOPPER_ITEM, HOPPER_STATE};
use crate::inventory::Stack;
use crate::server::entities::chest::ChestPayload;

fn resident(state: &mut State) {
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
}
fn advance(state: &mut State, tick: &mut u64, count: u64) {
    for _ in 0..count {
        *tick += 1;
        crate::server::runtime::tick_once(state, TickId::new(*tick), Instant::now()).unwrap();
        // Synthetic ticks run faster than fsync. Observe committed receipts
        // before advancing again rather than racing the real journal worker.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !state.durability.pending.is_empty() {
            assert!(Instant::now() < deadline, "journal did not settle");
            crate::server::durable::receipt::poll_journal_receipts(state).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
fn contents(state: &State, y: i32) -> ChestPayload {
    let id = state
        .entities
        .anchored_at(CellCoord::new(-1, y, -1))
        .unwrap();
    state
        .entities
        .snapshot(id)
        .unwrap()
        .private_payload
        .downcast_ref::<ChestPayload>()
        .unwrap()
        .clone()
}
fn transfer(
    state: &State,
    action_id: u128,
    y: i32,
    operation: u8,
    slot: u8,
    player: u8,
    count: u16,
) -> ClientMessage {
    let id = state
        .entities
        .anchored_at(CellCoord::new(-1, y, -1))
        .unwrap();
    let snapshot = state.entities.snapshot(id).unwrap();
    let mut payload = vec![2, operation, slot, player];
    payload.extend(count.to_le_bytes());
    payload.extend(id.get().to_le_bytes());
    payload.extend(snapshot.revision.to_le_bytes());
    ClientMessage::EntityInteract {
        action_id,
        target: [-1, y, -1],
        payload,
    }
}

#[test]
fn chest_hopper_chest_chain_preserves_last_slot_components_restart_and_refunds() {
    let path = temp_save_dir("chest-automation");
    let mut state = server_state(53, path.clone()).unwrap();
    resident(&mut state);
    let tagged = Stack::with_components(ItemId(crate::world::STONE.0), 6, 1, vec![42, 7]).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(CHEST_ITEM, 2));
    inventory.slots[1] = Some(Stack::new(HOPPER_ITEM, 1));
    inventory.slots[25] = Some(tagged.clone());
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
    let epoch = grant_action_epoch(&mut state, 17);
    let mut seq = 0u128;
    let mut next = || {
        seq += 1;
        (u128::from(epoch) << 64) | seq
    };
    let mut tick = 40_000;
    for (y, block, slot) in [
        (78, CHEST_STATE, 0),
        (79, HOPPER_STATE, 1),
        (80, CHEST_STATE, 0),
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
    let message = transfer(&state, next(), 80, 0, 26, 25, 6);
    settle_live_action(&mut state, tick, message);
    tick += 1;
    assert_eq!(contents(&state, 80).slots[26], Some(tagged.clone()));
    advance(&mut state, &mut tick, 80);
    drop(peer);
    drop(state);
    let mut state = server_state(53, path.clone()).unwrap();
    resident(&mut state);
    tick = state.recovered_tick;
    advance(&mut state, &mut tick, 400);
    assert!(contents(&state, 80).slots.iter().all(Option::is_none));
    assert_eq!(contents(&state, 78).slots[0], Some(tagged.clone()));
    assert!(contents(&state, 78).slots[1..].iter().all(Option::is_none));
    for y in [78, 80] {
        let id = state
            .entities
            .anchored_at(CellCoord::new(-1, y, -1))
            .unwrap();
        assert_eq!(
            state.entities.snapshot(id).unwrap().next_tick,
            None,
            "chests are passive"
        );
    }
    let inventory = state.inventory_store.load(17).unwrap();
    let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
    let epoch = grant_action_epoch(&mut state, 17);
    let message = transfer(&state, (u128::from(epoch) << 64) | 1, 78, 1, 0, 26, 2);
    settle_live_action(&mut state, tick + 1, message.clone());
    settle_live_action(&mut state, tick + 2, message);
    assert_eq!(
        state.clients[&1].inventory.slots[26]
            .as_ref()
            .unwrap()
            .count,
        2,
        "retry cannot collect twice"
    );
    settle_live_action(
        &mut state,
        tick + 3,
        ClientMessage::Edit {
            action_id: (u128::from(epoch) << 64) | 2,
            x: -1,
            y: 78,
            z: -1,
            block: AIR,
            slot: 0,
        },
    );
    assert!(
        state
            .entities
            .anchored_at(CellCoord::new(-1, 78, -1))
            .is_none()
    );
    let drops = crate::server::drops::nearby(&state.entities, [-0.5, 78.5, -0.5]);
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
    assert!(stacks.iter().any(|s| s.item == CHEST_ITEM && s.count == 1));
    assert!(
        stacks
            .iter()
            .any(|s| s.item == tagged.item && s.count == 4 && s.components == tagged.components)
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
