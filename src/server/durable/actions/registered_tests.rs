use super::*;
use crate::inventory::{STACK_LIMIT, Stack};
use crate::server::entities::{EntityPayload, EntitySpawn};
use bloxgloom_host_api::actions::Request;

pub(super) fn register_probe(catalog: &mut crate::content::Catalog, entity: &str) {
    use bloxgloom_host_api::actions::*;
    catalog
        .register_action(Action {
            key: format!("{entity}/probe"),
            version: 1,
            label: "PROBE".into(),
            target: Target::Entity(entity.into()),
            operation: Operation::EntityRequest(vec![0]),
            panel: None,
        })
        .unwrap();
}
pub(super) fn probe_request(entity: &str, id: crate::server::entities::EntityId) -> Vec<u8> {
    Request {
        key: format!("{entity}/probe"),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: id.get(),
        entity_revision: 1,
        arguments: vec![],
    }
    .encode()
    .unwrap()
}

#[test]
fn action_without_an_authoritative_handler_fails_before_world_creation() {
    struct Unsupported;
    impl bloxgloom_host_api::Extension for Unsupported {
        fn register(
            &self,
            r: &mut dyn bloxgloom_host_api::Registrar,
        ) -> Result<(), bloxgloom_host_api::RegistrationError> {
            use bloxgloom_host_api::actions::*;
            r.action(Action {
                key: "test:unsupported".into(),
                version: 1,
                label: "UNSUPPORTED".into(),
                target: Target::Block("bloxgloom:stone".into()),
                operation: Operation::EntityRequest(vec![0]),
                panel: None,
            })
        }
    }
    let path = temp_save_dir("unsupported-action-handler");
    let startup = crate::server::startup::ServerStartup::new(std::sync::Arc::new(
        crate::content::Catalog::builtins(),
    ))
    .with_extension(&Unsupported)
    .unwrap();
    assert!(crate::server::server_state_with_startup(31, path.clone(), 8, startup).is_err());
    assert!(
        !path.exists(),
        "invalid action contracts cannot create save data"
    );
}

#[test]
fn registered_recipe_rejects_full_output_components_stale_and_malformed_without_partial_debit() {
    let path = temp_save_dir("action-validation");
    let startup = crate::server::startup::ServerStartup::new(std::sync::Arc::new(
        crate::content::Catalog::builtins(),
    ))
    .with_extension(&bloxgloom_lifecycle_fixture::Fixture)
    .unwrap();
    let mut state = crate::server::server_state_with_startup(31, path.clone(), 8, startup).unwrap();
    let gravel = ItemId(crate::world::GRAVEL.0);
    let mut inventory = Inventory {
        slots: std::array::from_fn(|_| Some(Stack::new(STICK, STACK_LIMIT))),
        revision: 7,
    };
    inventory.slots[0] = Some(Stack::new(gravel, 3));
    let peer = add_test_client(&mut state, [0.0, 80.0, 0.0], inventory.clone());
    let base = Request {
        key: bloxgloom_lifecycle_fixture::actions::KEY.into(),
        version: 1,
        slot: 0,
        inventory_revision: 7,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    let plan = |state: &mut State, request: &Request| {
        plan_durable_request(
            state,
            &edit_request(ClientMessage::EntityInteract {
                action_id: 1,
                target: [0, 80, 0],
                payload: request.encode().unwrap(),
            }),
            TickId::new(1),
        )
    };
    assert!(
        plan(&mut state, &base).is_err(),
        "full output must reject entire recipe"
    );
    assert_eq!(state.clients[&1].inventory, inventory);
    state.clients.get_mut(&1).unwrap().inventory.slots[1] = None;
    for mutate in [0, 1, 2, 3, 4] {
        let mut request = base.clone();
        match mutate {
            0 => request.inventory_revision = 6,
            1 => request.slot = 36,
            2 => request.version = 2,
            3 => request.entity = 1,
            _ => request.arguments.push(1),
        }
        assert!(plan(&mut state, &request).is_err());
    }
    let with_space = state.clients[&1].inventory.clone();
    state.clients.get_mut(&1).unwrap().inventory.slots[0] =
        Some(Stack::with_components(gravel, 3, 1, vec![9]).unwrap());
    assert!(
        plan(&mut state, &base).is_err(),
        "components must not be destroyed by plain recipe"
    );
    state.clients.get_mut(&1).unwrap().inventory = with_space.clone();
    let action = plan(&mut state, &base).unwrap().unwrap();
    assert_eq!(
        state.clients[&1].inventory, with_space,
        "planning cannot mutate inventory before receipt"
    );
    let after = action.inventory.unwrap();
    assert_eq!(after.slots[0], Some(Stack::new(gravel, 1)));
    assert_eq!(after.slots[1], Some(Stack::new(STICK, 3)));
    assert_eq!(after.revision, 8);
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn registered_inventory_checks_identity_revision_reach_visibility_and_fences_sight_chunks() {
    let path = temp_save_dir("registered-action-sight");
    let mut state = server_state(31, path.clone()).unwrap();
    let anchor = CellCoord::new(15, 80, 0);
    for x in 15..=18 {
        for y in 80..=82 {
            state.world.edit(x, y, 0, AIR).unwrap();
        }
    }
    state
        .world
        .edit(15, 80, 0, crate::content::CHEST_STATE)
        .unwrap();
    let prepared = state
        .entities
        .prepare_spawn(EntitySpawn::Anchored {
            entity_type: crate::content::CHEST_ENTITY_TYPE,
            anchor,
            anchor_state: crate::content::CHEST_STATE,
            footprint: vec![anchor],
            payload: EntityPayload::new(crate::server::entities::container::ContainerPayload {
                slots: vec![None; 27],
            }),
            spawn_tick: 1,
        })
        .unwrap();
    let id = prepared.entity_id();
    state.entities.apply_committed(prepared).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(STICK, 5));
    let peer = add_test_client(&mut state, [17.5, 80.0, 0.5], inventory.clone());
    let request = Request {
        key: "bloxgloom:chest/inventory".into(),
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: id.get(),
        entity_revision: state.entities.snapshot(id).unwrap().revision,
        arguments: vec![0, 0, 1, 0],
    };
    let plan = |state: &mut State, request: &Request, target| {
        plan_durable_request(
            state,
            &edit_request(ClientMessage::EntityInteract {
                action_id: 1,
                target,
                payload: request.encode().unwrap(),
            }),
            TickId::new(1),
        )
    };
    let action = plan(&mut state, &request, [15, 80, 0]).unwrap().unwrap();
    let actor_chunk = world_to_chunk(17, 81, 0).0;
    assert_ne!(actor_chunk, anchor.chunk());
    assert!(
        action
            .entities
            .as_ref()
            .unwrap()
            .read_keys()
            .any(|k| *k == super::super::super::chunk_state_key(actor_chunk)),
        "sight outside planner footprint must survive to admission/receipt fences"
    );
    assert_eq!(state.clients[&1].inventory, inventory);
    let mut stale = request.clone();
    stale.entity_revision += 1;
    assert!(plan(&mut state, &stale, [15, 80, 0]).is_err());
    stale = request.clone();
    stale.entity += 1;
    assert!(plan(&mut state, &stale, [15, 80, 0]).is_err());
    assert!(plan(&mut state, &request, [14, 80, 0]).is_err());
    state.world.edit(16, 81, 0, crate::world::STONE).unwrap();
    assert!(
        plan(&mut state, &request, [15, 80, 0]).is_err(),
        "forged click cannot pass a wall"
    );
    state.world.edit(16, 81, 0, AIR).unwrap();
    state.clients.get_mut(&1).unwrap().movement = MovementState::new([30.5, 80.0, 0.5], 0);
    assert!(
        plan(&mut state, &request, [15, 80, 0]).is_err(),
        "registered control is not reach authorization"
    );
    drop(peer);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
