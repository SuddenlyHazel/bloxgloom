use super::*;
use crate::inventory::Stack;

fn stage(state: &mut State, action: &CommitAction) -> Result<bool, super::super::StageError> {
    // Isolate terrain conflicts from the shared per-profile receipt key. The
    // normal planner still supplies all world/inventory before-values.
    let mut action = action.clone();
    action.action_id = None;
    action.receipt_value = None;
    let permit = action.entities.as_ref().map(|_| {
        state
            .durability
            .entity_mirror
            .try_reserve_durable()
            .unwrap()
            .unwrap()
    });
    state.durability.try_stage(TickId::new(1), &action, permit)
}
fn apply(state: &mut State) {
    crate::server::durable::complete_barrier(
        state,
        crate::server::durable::CommitBarrier::AllStaged,
    )
    .unwrap();
}

#[test]
fn cross_chunk_plant_and_support_reads_fence_pending_writes_and_stale_absence() {
    for place_first in [false, true] {
        let path = temp_save_dir(if place_first {
            "support-read-plant-first"
        } else {
            "support-read-soil-first"
        });
        let mut state = server_state(23, path.clone()).unwrap();
        let y = ((MAX_GENERATED_HEIGHT / 16) + 1) * 16;
        state.world.edit(0, y - 1, 0, GRASS).unwrap();
        state.world.edit(0, y, 0, crate::world::AIR).unwrap();
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::new(crate::items::ItemId::new(RED_FLOWER.get()), 1));
        let peer = add_test_client(&mut state, [0.5, y as f32, -2.5], inventory);
        let place = edit_request(ClientMessage::Edit {
            action_id: 1,
            x: 0,
            y,
            z: 0,
            block: RED_FLOWER,
            slot: 0,
        });
        let remove = edit_request(ClientMessage::Edit {
            action_id: 2,
            x: 0,
            y: y - 1,
            z: 0,
            block: crate::world::AIR,
            slot: 0,
        });
        let placement = plan_durable_request(&mut state, &place, TickId::new(1))
            .unwrap()
            .unwrap();
        let removal = plan_durable_request(&mut state, &remove, TickId::new(1))
            .unwrap()
            .unwrap();
        let (first, second) = if place_first {
            (&placement, &removal)
        } else {
            (&removal, &placement)
        };
        assert!(stage(&mut state, first).unwrap());
        assert!(matches!(
            stage(&mut state, second),
            Err(super::super::StageError::Conflict)
        ));
        // Reads are fenced before the first receipt; committed changes stale the
        // old stamp even once reservations are released (including absent plant).
        apply(&mut state);
        assert!(matches!(
            stage(&mut state, second),
            Err(super::super::StageError::Conflict)
        ));
        if place_first {
            let replanned = plan_durable_request(&mut state, &remove, TickId::new(2))
                .unwrap()
                .unwrap();
            assert_eq!(replanned.changed_cells.len(), 2);
            assert!(stage(&mut state, &replanned).unwrap());
            apply(&mut state);
        } else {
            assert!(
                matches!(plan_durable_request(&mut state,&place,TickId::new(2)),Err(e) if e.kind()==ErrorKind::PermissionDenied)
            );
            assert_eq!(
                state.clients[&1].inventory.slots[0].as_ref().unwrap().count,
                1
            );
        }
        assert_eq!(state.world.cached_block(0, y, 0), Some(crate::world::AIR));
        assert_eq!(
            state.world.cached_block(0, y - 1, 0),
            Some(crate::world::AIR)
        );
        drop(peer);
        drop(state);
        fs::remove_dir_all(path).unwrap();
    }
}
