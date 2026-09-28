//! A one-cell public proposal removes a complete storage footprint through the
//! production owner dispatcher, shared decisions, and the normal receipt gate.
use super::*;
use crate::inventory::Stack;
use crate::server::entities::container::ContainerPayload;
use crate::server::entities::{CellCoord, EntityId, EntityPatch, EntityPayload, EntitySpawn};
use crate::world::{GRASS, RED_FLOWER};

const STORE: &str = bloxgloom_lifecycle_fixture::KEY;

struct StorageOwner {
    touched: [i32; 3],
}
impl system::Behavior for StorageOwner {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        system::Behavior::validate(&Behavior, data)
    }
    fn plan(&self, c: &system::Context<'_>) -> Result<system::Plan, RegistrationError> {
        Ok(system::Plan {
            data: vec![c.data[0] + 1],
            next_tick: c.tick + 100,
            wakes: vec![],
            edits: if c.data == [0] {
                vec![system::BlockEdit {
                    cell: self.touched,
                    before: c
                        .block(self.touched)
                        .map_err(|e| RegistrationError(e.to_string()))?
                        .state,
                    after: "bloxgloom:air".into(),
                }]
            } else {
                vec![]
            },
        })
    }
}

struct StorageRemoved;
impl api::Handler for StorageRemoved {
    fn handle(&self, c: &mut api::Context<'_>, event: &api::Event) -> Result<(), api::Error> {
        let api::Event::BlockRemoved {
            cell,
            cause: api::RemovalCause::AnchoredBreak,
            ..
        } = event
        else {
            return Err(api::Error::Invalid("expected one anchored removal".into()));
        };
        let position = cell.map(|n| n as f32 + 0.5);
        // The lifecycle supplies the storage item, not the harvest fallback.
        c.spawn_drop(position, "bloxgloom:stick", 1, 250)?;
        c.spawn_entity("test:marker", position, &[1])
    }
}
struct StorageNeighbor;
impl api::Handler for StorageNeighbor {
    fn handle(&self, c: &mut api::Context<'_>, event: &api::Event) -> Result<(), api::Error> {
        if let api::Event::NeighborChanged {
            cell,
            previous,
            current,
            ..
        } = event
            && previous.block_type == STORE
            && current.block_type == "bloxgloom:air"
        {
            c.set_block(*cell, "bloxgloom:air")?;
        }
        Ok(())
    }
}
impl Extension for StorageOwner {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        bloxgloom_lifecycle_fixture::TallStore.register(r)?;
        r.owner_system(system::System {
            key: KEY.into(),
            schema: 1,
            partition: system::Partition::Chunk,
            max_state_bytes: 1,
            max_jobs_per_tick: 1,
            read_radius_chunks: Some(0),
            after: vec![],
            seeds: vec![system::Seed {
                owner: system::Owner::Chunk([8, 6, 0]),
                data: vec![0],
            }],
            behavior: Arc::new(StorageOwner {
                touched: self.touched,
            }),
        })?;
        r.gameplay_entity(api::EntityDefinition {
            key: "test:marker".into(),
            schema_version: 1,
            schema_fingerprint: 123,
            max_state_bytes: 1,
            initial_delay_ticks: None,
            state: Arc::new(Marker),
        })?;
        r.gameplay_handler(api::HandlerRegistration {
            key: "test:storage_removed".into(),
            version: 1,
            event: api::EventKind::BlockRemoved,
            target: Some(STORE.into()),
            handler: Arc::new(StorageRemoved),
        })?;
        r.gameplay_handler(api::HandlerRegistration {
            key: "test:storage_neighbor".into(),
            version: 1,
            event: api::EventKind::NeighborChanged,
            target: Some("bloxgloom:grass".into()),
            handler: Arc::new(StorageNeighbor),
        })
    }
}
fn storage_startup(touched: [i32; 3]) -> ServerStartup {
    ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&StorageOwner { touched })
        .unwrap()
}
fn seed_storage(state: &mut State, touched: [i32; 3]) -> EntityId {
    let [x, y, z] = touched;
    let anchor = CellCoord::new(x, y - 1, z);
    for cy in [y - 1, y] {
        state.world.get_block(x, cy, z).unwrap();
    }
    let block = state.world.catalog().state_by_key(STORE).unwrap();
    let entity_type = state.world.catalog().entity_type_id_by_key(STORE).unwrap();
    let mut slots = vec![None; 9];
    slots[0] = Some(Stack::new(crate::items::STICK, 7));
    let spawn = EntitySpawn::Anchored {
        entity_type,
        anchor,
        anchor_state: block,
        footprint: vec![anchor, CellCoord::new(x, y, z)],
        payload: EntityPayload::new(ContainerPayload { slots }),
        spawn_tick: 1,
    };
    let mut action = empty_action();
    action.world_edits = state
        .world
        .prepare_edits(&[
            (x, y - 1, z, block),
            (x, y, z, block),
            (x + 1, y - 1, z, GRASS),
            (x + 1, y, z, RED_FLOWER),
        ])
        .unwrap();
    action.entities = crate::server::drops::plan_stack_spawns_with_extra(
        &state.entities,
        state.world.catalog(),
        &[],
        vec![spawn],
        1,
        crate::server::drops::unix_ms(),
    )
    .unwrap();
    assert!(stage_action(state, &action).unwrap());
    complete_barrier(state, CommitBarrier::AllStaged).unwrap();
    state.durability.publish_queue.clear();
    state.entities.anchored_at(anchor).unwrap()
}
fn unchanged(state: &State, id: EntityId) {
    assert_eq!(owner(state), (0, vec![0]));
    assert!(state.entities.snapshot(id).is_some());
    assert_eq!(drop_count(state), 0);
    assert!(state.durability.publish_queue.is_empty());
    assert_eq!(state.world.cached_block(137, 100, 8), Some(RED_FLOWER));
}
fn removed(state: &mut State, id: EntityId, contents: u16) {
    assert_eq!(owner(state), (1, vec![1]));
    assert!(state.entities.snapshot(id).is_none());
    for [x, y, z] in [[136, 99, 8], CELL, [137, 99, 8], [137, 100, 8]] {
        assert_eq!(state.world.get_block(x, y, z).unwrap(), AIR);
        assert!(
            state
                .entities
                .anchored_at(CellCoord::new(x, y, z))
                .is_none()
        );
    }
    let drops = crate::server::drops::nearby(&state.entities, POSITION);
    let count = |item| {
        drops
            .iter()
            .filter(|drop| drop.item == item)
            .map(|drop| drop.count)
            .sum::<u16>()
    };
    let catalog = state.world.catalog();
    let item = |key| catalog.item_by_key(key).unwrap();
    assert_eq!(
        count(item(STORE)),
        1,
        "one refund, not one per footprint cell"
    );
    assert_eq!(
        count(crate::items::STICK),
        contents + 1,
        "exact contents and one callback"
    );
    assert_eq!(
        count(item("bloxgloom:grass")),
        1,
        "expanded-cell neighbor removal runs once"
    );
    assert_eq!(
        count(item("bloxgloom:red_flower")),
        1,
        "support loss runs once"
    );
    assert_eq!(
        drops.iter().map(|drop| drop.count).sum::<u16>(),
        contents + 4
    );
    let marker_type = catalog.entity_type_id_by_key("test:marker").unwrap();
    assert_eq!(
        state
            .entities
            .query_mobile_aabb([135.0, 98.0, 7.0], [139.0, 102.0, 10.0])
            .unwrap()
            .into_iter()
            .filter(|id| state.entities.snapshot(*id).unwrap().entity_type == marker_type)
            .count(),
        1
    );
}

#[test]
fn owner_storage_expands_once_preserves_contents_and_recovers_one_wal_record() {
    for lose_receipt in [false, true] {
        let path = save();
        let open = || server_state_with_startup(7, path.clone(), 2, storage_startup(CELL)).unwrap();
        let mut state = open();
        assert!(stage(&mut state, 1).unwrap().is_none());
        let id = seed_storage(&mut state, CELL);
        // A queued contents edit must win; retry must refund the new snapshot.
        let before = state.entities.snapshot(id).unwrap();
        let mut payload = before
            .private_payload
            .downcast_ref::<ContainerPayload>()
            .unwrap()
            .clone();
        payload.slots[0] = Some(Stack::new(crate::items::STICK, 8));
        let mut update = empty_action();
        update.entities = Some(
            state
                .entities
                .prepare_update(
                    id,
                    before.revision,
                    EntityPatch {
                        payload: Some(EntityPayload::new(payload)),
                        ..Default::default()
                    },
                )
                .unwrap(),
        );
        assert!(stage_action(&mut state, &update).unwrap());
        assert_eq!(
            stage(&mut state, 2).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        unchanged(&state, id);
        complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
        state.durability.publish_queue.clear();
        state.durability.rotation_requested = true;
        assert_eq!(
            stage(&mut state, 3).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        state.durability.rotation_requested = false;
        unchanged(&state, id);
        let next = state.durability.next_id;
        let wave = stage(&mut state, 4).unwrap().unwrap();
        assert_eq!(state.durability.next_id, next + 1);
        assert_eq!(state.durability.pending.len(), 1);
        unchanged(&state, id);
        assert!(matches!(
            stage_action(&mut state, &update),
            Err(StageError::Conflict)
        ));
        if lose_receipt {
            let (sender, receiver) = std::sync::mpsc::channel();
            let real = std::mem::replace(&mut state.durability.pending[0].receiver, receiver);
            real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            drop(sender);
            assert!(complete_barrier(&mut state, wave.barrier()).is_err());
            unchanged(&state, id);
        } else {
            complete_barrier(&mut state, wave.barrier()).unwrap();
            removed(&mut state, id, 8);
        }
        drop(state);
        let mut state = open();
        removed(&mut state, id, 8);
        assert!(stage(&mut state, 5).unwrap().is_none());
        removed(&mut state, id, 8);
        drop(state);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn owner_storage_rejects_out_of_radius_or_changed_footprint_without_partial_removal() {
    for outside_radius in [false, true] {
        let path = save();
        let touched = if outside_radius { [136, 96, 8] } else { CELL };
        let mut state =
            server_state_with_startup(7, path.clone(), 1, storage_startup(touched)).unwrap();
        let id = seed_storage(&mut state, touched);
        if !outside_radius {
            let mut change = empty_action();
            change.world_edits = state.world.prepare_edits(&[(136, 99, 8, SAND)]).unwrap();
            assert!(stage_action(&mut state, &change).unwrap());
            complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
        }
        let next = state.durability.next_id;
        let error = stage(&mut state, 2).unwrap_err();
        assert_eq!(
            error.kind(),
            if outside_radius {
                ErrorKind::Unsupported
            } else {
                ErrorKind::WouldBlock
            }
        );
        assert_eq!(state.durability.next_id, next);
        assert!(state.durability.pending.is_empty());
        assert_eq!(owner(&state), (0, vec![0]));
        assert!(state.entities.snapshot(id).is_some());
        assert_eq!(drop_count(&state), 0);
        assert_eq!(
            state.world.cached_block(touched[0], touched[1], touched[2]),
            Some(state.world.catalog().state_by_key(STORE).unwrap())
        );
        drop(state);
        std::fs::remove_dir_all(path).unwrap();
    }
}
