use super::*;
use crate::server::{entities::*, startup::ServerStartup};
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, entity as api};
use std::sync::Arc;

struct Replace;
impl api::Behavior for Replace {
    fn initial(&self) -> api::Payload {
        api::Payload::new(())
    }
    fn decode(&self, b: &[u8]) -> Result<api::Payload, api::Error> {
        if b == [0] {
            Ok(self.initial())
        } else {
            Err(api::Error::InvalidState)
        }
    }
    fn encode(&self, _: &api::Payload) -> Result<Vec<u8>, api::Error> {
        Ok(vec![0])
    }
    fn public(&self, _: &api::Payload) -> Result<Vec<u8>, api::Error> {
        Ok(vec![0])
    }
    fn pose(&self, _: &[u8]) -> Result<api::Pose, api::Error> {
        Ok(api::Pose {
            yaw: 0.0,
            grounded: true,
        })
    }
    fn tick(&self, c: &api::Context<'_>) -> Result<api::Plan, api::Error> {
        let child = bloxgloom_lifecycle_fixture::creature::definition();
        Ok(api::Plan {
            state: None,
            next_tick: None,
            position: None,
            lifecycle: api::Lifecycle {
                despawn: true,
                spawns: vec![api::Spawn {
                    key: child.key,
                    position: [c.position[0] + 1.0, c.position[1], c.position[2]],
                    state: child.behavior.initial(),
                }],
            },
        })
    }
}
struct Package;
impl Extension for Package {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        let mut parent = bloxgloom_lifecycle_fixture::creature::definition();
        parent.key = "fixture:parent".into();
        parent.max_state_bytes = 1;
        parent.max_public_bytes = 1;
        parent.behavior = Arc::new(Replace);
        parent.interaction.clear();
        r.mobile_entity(parent)?;
        r.mobile_entity(bloxgloom_lifecycle_fixture::creature::definition())
    }
}
#[test]
fn public_spawn_and_self_removal_are_one_atomic_recoverable_transaction() {
    let path = temp_save_dir("mobile-public-effects");
    let startup = || {
        ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&Package)
            .unwrap()
    };
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 8, startup()).unwrap();
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
    for x in 0..=4 {
        for z in 0..=4 {
            for y in 79..=83 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 79 {
                            crate::world::STONE
                        } else {
                            crate::world::AIR
                        },
                    )
                    .unwrap();
            }
        }
    }
    let catalog = state.world.catalog_arc();
    let parent = catalog.entity_type_id_by_key("fixture:parent").unwrap();
    let id = stage_entity_spawn(
        &mut state,
        EntitySpawn::Mobile {
            entity_type: parent,
            position: [1.5, 80.0, 1.5],
            payload: catalog.mobile_entity(parent).unwrap().behavior.initial(),
            spawn_tick: 0,
        },
    );
    // A captured terrain edit must fence the entire replacement, including the
    // allocation. No child may escape a stale parent plan.
    let input = super::super::entity::capture_tick_input(&mut state, id, 1, false)
        .unwrap()
        .unwrap();
    let plan = input.plan().unwrap();
    state.world.edit(2, 80, 1, crate::world::STONE).unwrap();
    assert_eq!(
        super::super::entity::commit_tick_plan(&mut state, input, plan)
            .err()
            .unwrap()
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(state.entities.snapshot(id).is_some());
    assert!(super::super::entity::plan_entity_tick(&mut state, id, 2, false).is_err());
    state.world.edit(2, 80, 1, crate::world::AIR).unwrap();
    let action = super::super::entity::plan_entity_tick(&mut state, id, 3, false)
        .unwrap()
        .unwrap();
    assert!(
        state.entities.snapshot(id).is_some(),
        "planning must not publish removal"
    );
    settle_commit_action(&mut state, &action, 3);
    assert!(state.entities.snapshot(id).is_none());
    let child_type = catalog
        .entity_type_id_by_key(bloxgloom_lifecycle_fixture::creature::KEY)
        .unwrap();
    let key = world_to_chunk(2, 80, 1).0;
    let children = state
        .entities
        .public_views_for_chunk_bounded(key, 10)
        .unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].entity_type, child_type);
    let child_id = children[0].id;
    drop(state);
    let recovered =
        crate::server::server_state_with_startup(7, path.clone(), 8, startup()).unwrap();
    assert!(recovered.entities.snapshot(id).is_none());
    assert!(recovered.entities.snapshot(child_id).is_some());
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}
