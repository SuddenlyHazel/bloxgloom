use super::*;
use crate::inventory::Stack;
use crate::{
    content::{HOPPER_ENTITY_TYPE, HOPPER_STATE},
    server::{
        entities::{EntityPayload, EntitySpawn, machine::MachinePayload},
        startup::ServerStartup,
    },
};
use std::sync::Arc;
#[path = "machine_component_tests.rs"]
mod components;

#[test]
fn public_machine_lifecycle_matches_runtime_footprint_and_rejects_stale_removal() {
    let catalog = Arc::new(crate::content::Catalog::builtins());
    let machine = catalog.machine(crate::content::KILN_ENTITY_TYPE).unwrap();
    let anchor = [7, 80, -3];
    let key = &machine.variants[1].placement_state;
    let placed = machine.plan_place(anchor, key).unwrap();
    assert_eq!(placed.variant, 1);
    assert_eq!(placed.item, machine.item);
    let adapter = crate::server::entities::machine::Adapter::new(catalog.clone(), machine.clone());
    let expected = adapter
        .cells(
            CellCoord::new(anchor[0], anchor[1], anchor[2]),
            &MachinePayload::empty(machine.slots, 1),
        )
        .unwrap();
    assert_eq!(
        placed
            .cells
            .iter()
            .map(|(at, key)| (
                CellCoord::new(at[0], at[1], at[2]),
                catalog.state_by_key(key).unwrap()
            ))
            .collect::<Vec<_>>(),
        expected
    );
    let stored: Vec<_> = placed.cells.iter().map(|(at, _)| *at).collect();
    let active = machine
        .plan_remove(anchor, [7, 81, -3], 1, true, &stored)
        .unwrap();
    assert_ne!(active.cells, placed.cells);
    let mut payload = MachinePayload::empty(machine.slots, 1);
    payload.fuel = 1;
    assert_eq!(
        active
            .cells
            .iter()
            .map(|(at, key)| (
                CellCoord::new(at[0], at[1], at[2]),
                catalog.state_by_key(key).unwrap()
            ))
            .collect::<Vec<_>>(),
        adapter.cells(CellCoord::new(7, 80, -3), &payload).unwrap()
    );
    assert!(
        machine
            .plan_remove(anchor, [7, 81, -3], 1, true, &stored[..1])
            .is_err()
    );
    assert!(
        machine
            .plan_remove(anchor, [8, 81, -3], 1, true, &stored)
            .is_err()
    );
    assert!(machine.plan_place([0, i32::MAX, 0], key).is_err());
}

#[test]
fn registered_machine_ports_reject_wrong_faces_and_forged_destination_without_item_changes() {
    let path = temp_save_dir("machine-port-validation");
    let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&bloxgloom_lifecycle_fixture::Fixture)
        .unwrap();
    let mut state = crate::server::server_state_with_startup(53, path.clone(), 8, startup).unwrap();
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
    let catalog = state.world.catalog_arc();
    let key = bloxgloom_lifecycle_fixture::machine::KEY;
    let crusher = catalog.entity_type_id_by_key(key).unwrap();
    let block = catalog.state_by_key(key).unwrap();
    let item = ItemId(crate::world::STONE.0);
    let mut ids = vec![];
    for (y, entity_type, block, slots) in [
        (80, crusher, block, vec![None; 3]),
        (
            81,
            HOPPER_ENTITY_TYPE,
            HOPPER_STATE,
            vec![Some(Stack::new(item, 2)), None, None],
        ),
    ] {
        state.world.edit(0, y, 0, block).unwrap();
        let anchor = CellCoord::new(0, y, 0);
        ids.push(stage_entity_spawn(
            &mut state,
            EntitySpawn::Anchored {
                entity_type,
                anchor,
                anchor_state: block,
                footprint: vec![anchor],
                payload: EntityPayload::new(MachinePayload {
                    slots,
                    ..MachinePayload::empty(3, 0)
                }),
                spawn_tick: 0,
            },
        ));
    }
    let input = super::super::entity::capture_tick_input(&mut state, ids[1], 21, false)
        .unwrap()
        .unwrap();
    let mut plan = input.plan().unwrap();
    assert!(plan.transfer.is_some());
    plan.transfer
        .as_mut()
        .unwrap()
        .route
        .as_mut()
        .unwrap()
        .destination = 1; // output port cannot accept from above
    assert!(super::super::entity::commit_tick_plan(&mut state, input, plan).is_err());
    assert_eq!(
        state
            .entities
            .snapshot(ids[1])
            .unwrap()
            .private_payload
            .downcast_ref::<MachinePayload>()
            .unwrap()
            .slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    assert!(
        state
            .entities
            .snapshot(ids[0])
            .unwrap()
            .private_payload
            .downcast_ref::<MachinePayload>()
            .unwrap()
            .slots
            .iter()
            .all(Option::is_none)
    );
    let action = super::super::entity::plan_entity_tick(&mut state, ids[1], 21, false)
        .unwrap()
        .unwrap();
    settle_commit_action(&mut state, &action, 21);
    assert_eq!(
        state
            .entities
            .snapshot(ids[0])
            .unwrap()
            .private_payload
            .downcast_ref::<MachinePayload>()
            .unwrap()
            .slots[1],
        Some(Stack::new(item, 1))
    );
    let descriptor = state.entities.types().descriptor(crusher).unwrap();
    let port = descriptor.transfer_policy().unwrap();
    assert!(port.port(0, [1, 0, 0]).is_none());
    assert!(port.port(1, [0, 1, 0]).is_none());
    let target = state.entities.snapshot(ids[0]).unwrap().private_payload;
    let fuel = Stack::with_components(STICK, 1, 1, vec![9]).unwrap();
    assert!(
        port.port(0, [0, 1, 0])
            .unwrap()
            .deposit(&target, &fuel, &catalog)
            .unwrap()
            .is_none()
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
