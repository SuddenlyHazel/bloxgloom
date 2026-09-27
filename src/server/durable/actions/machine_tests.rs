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
