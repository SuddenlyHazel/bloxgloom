//! Exercise public owner dispatch, not a hand-built participant payload.
use super::*;
use crate::server::parallel::OwnerKey;
use crate::server::registry::SystemId;
use crate::server::runtime::systems::{
    PendingRegisteredWave, RegisteredWaveInputs, RegisteredWorldInputs,
};
use crate::server::{State, server_state_with_startup, startup::ServerStartup};
use crate::world::{AIR, ChunkKey, SAND};
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, gameplay as api, system};
use std::sync::Arc;
use std::time::Duration;

mod burn;

const CELL: [i32; 3] = [136, 100, 8];
const POSITION: [f32; 3] = [136.5, 100.5, 8.5];
const KEY: &str = "test:participant_owner";

struct Fixture;
struct Behavior;
struct Marker;
struct Harvest;
impl system::Behavior for Behavior {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() == 1 {
            Ok(())
        } else {
            Err(RegistrationError("one byte required".into()))
        }
    }
    fn plan(&self, context: &system::Context<'_>) -> Result<system::Plan, RegistrationError> {
        let block = context
            .block(CELL)
            .map_err(|e| RegistrationError(e.to_string()))?;
        Ok(system::Plan {
            data: vec![context.data[0] + 1],
            next_tick: context.tick + 2,
            wakes: vec![],
            edits: vec![system::BlockEdit {
                cell: CELL,
                before: block.state,
                after: "bloxgloom:air".into(),
            }],
        })
    }
}
impl api::EntityState for Marker {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        system::Behavior::validate(&Behavior, data)
    }
    fn public(&self, data: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(data.to_vec())
    }
}
impl api::Handler for Harvest {
    fn handle(&self, context: &mut api::Context<'_>, event: &api::Event) -> Result<(), api::Error> {
        if matches!(event, api::Event::EntityTick { .. }) {
            return Ok(());
        }
        let api::Event::BlockRemoved {
            cause: api::RemovalCause::WorldEdit,
            ..
        } = event
        else {
            return Err(api::Error::Invalid("expected owner removal".into()));
        };
        let markers = context.nearby_entities(POSITION, 2.0)?;
        // A read-only neighboring population participates even though all
        // resulting writes remain in the source chunk.
        context.nearby_entities([152.5, 100.5, 8.5], 1.0)?;
        if let Some(marker) = markers.iter().find(|e| e.entity_type == "test:marker") {
            let before = context.entity_state(marker.id)?.unwrap();
            context.update_entity(marker.id, &[before[0] + 1])?;
            context.schedule_entity(marker.id, Some(20))?;
        } else {
            context.spawn_entity("test:marker", POSITION, &[1])?;
        }
        context.spawn_drop(POSITION, "bloxgloom:stick", 1, 250)
    }
}
impl Extension for Fixture {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.owner_system(system::System {
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
            behavior: Arc::new(Behavior),
        })?;
        registrar.gameplay_entity(api::EntityDefinition {
            key: "test:marker".into(),
            schema_version: 1,
            schema_fingerprint: 123,
            max_state_bytes: 1,
            initial_delay_ticks: Some(50),
            state: Arc::new(Marker),
        })?;
        for (key, event, target) in [
            (
                "test:owner_harvest",
                api::EventKind::BlockRemoved,
                "bloxgloom:sand",
            ),
            (
                "test:marker_tick",
                api::EventKind::EntityTick,
                "test:marker",
            ),
        ] {
            registrar.gameplay_handler(api::HandlerRegistration {
                key: key.into(),
                version: 1,
                event,
                target: Some(target.into()),
                handler: Arc::new(Harvest),
            })?;
        }
        Ok(())
    }
}
fn startup() -> ServerStartup {
    ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&Fixture)
        .unwrap()
}
fn save() -> std::path::PathBuf {
    static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "bloxgloom-owner-participants-{}-{}-{}",
        std::process::id(),
        NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn empty_action() -> CommitAction {
    CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: vec![],
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        entity_wakes: vec![],
        entities: None,
    }
}
fn stage_action(state: &mut State, action: &CommitAction) -> Result<bool, StageError> {
    let permit = action.entities.as_ref().map(|_| {
        state
            .durability
            .entity_mirror
            .try_reserve_durable()
            .unwrap()
            .unwrap()
    });
    state.durability.try_stage(TickId::new(1), action, permit)
}
fn plant(state: &mut State) {
    state.world.get_block(CELL[0], CELL[1], CELL[2]).unwrap();
    let mut action = empty_action();
    action.world_edits = state
        .world
        .prepare_edits(&[(CELL[0], CELL[1], CELL[2], SAND)])
        .unwrap();
    assert!(stage_action(state, &action).unwrap());
    complete_barrier(state, CommitBarrier::AllStaged).unwrap();
    state.durability.publish_queue.clear();
}
fn stage(state: &mut State, tick: u64) -> io::Result<Option<PendingRegisteredWave>> {
    let registered = state
        .phase_plan
        .system(&SystemId::new(KEY).unwrap())
        .unwrap()
        .clone();
    state.system_runtime.stage_registered_wave_with_world(
        &registered,
        TickId::new(tick),
        0,
        RegisteredWaveInputs {
            effects: &state.effect_kinds,
            durability: &mut state.durability,
            in_flight: &[],
            world: RegisteredWorldInputs {
                world: Some(&mut state.world),
                entities: Some(&state.entities),
                players: &[],
                seed: state.seed,
                missing: &mut vec![],
            },
        },
    )
}
fn owner(state: &State) -> (u64, Vec<u8>) {
    state
        .system_runtime
        .owner_value(
            &SystemId::new(KEY).unwrap(),
            OwnerKey::Chunk(ChunkKey { x: 8, y: 6, z: 0 }),
        )
        .unwrap()
}
fn marker(state: &State) -> crate::server::entities::EntitySnapshot {
    let kind = state
        .world
        .catalog()
        .entity_type_id_by_key("test:marker")
        .unwrap();
    state
        .entities
        .query_mobile_aabb([136.0, 100.0, 8.0], [137.0, 101.0, 9.0])
        .unwrap()
        .into_iter()
        .filter_map(|id| state.entities.snapshot(id))
        .find(|e| e.entity_type == kind)
        .unwrap()
}
fn drop_count(state: &State) -> u16 {
    crate::server::drops::nearby(&state.entities, POSITION)
        .iter()
        .map(|drop| drop.count)
        .sum()
}

#[test]
fn owner_participants_publish_together_and_recover_updates_merge_and_due_index() {
    let path = save();
    let mut state = server_state_with_startup(7, path.clone(), 2, startup()).unwrap();
    assert!(
        stage(&mut state, 1).unwrap().is_none(),
        "unavailable terrain must not become air"
    );
    assert_eq!(owner(&state), (0, vec![0]));
    plant(&mut state);
    let first_id = state.entities.next_id();
    state.durability.expire_queued = true;
    let wave = stage(&mut state, 2).unwrap().unwrap();
    assert_eq!(
        state.world.cached_block(CELL[0], CELL[1], CELL[2]),
        Some(SAND)
    );
    assert_eq!(owner(&state), (0, vec![0]));
    assert_eq!(state.entities.next_id(), first_id);
    assert!(state.durability.publish_queue.is_empty());
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(owner(&state), (1, vec![1]));
    assert_eq!(drop_count(&state), 1);
    assert!(
        state.durability.expire_queued,
        "owner publication must not complete an unrelated expiry sweep"
    );
    let effect = state.durability.publish_queue.last().unwrap();
    assert_eq!(effect.deltas.len(), 1);
    assert_eq!(effect.entity_commit.as_ref().unwrap().deltas.len(), 2);
    let id = marker(&state).id;
    assert_eq!(marker(&state).next_tick, Some(52));
    plant(&mut state);
    let wave = stage(&mut state, 4).unwrap().unwrap();
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(owner(&state), (2, vec![2]));
    assert_eq!(drop_count(&state), 2);
    assert_eq!(marker(&state).id, id);
    assert_eq!(marker(&state).next_tick, Some(24));
    assert_eq!(
        state.entities.next_id(),
        first_id + 2,
        "merge/update must not allocate"
    );
    drop(state);
    let mut state = server_state_with_startup(7, path.clone(), 2, startup()).unwrap();
    assert_eq!(owner(&state), (2, vec![2]));
    assert_eq!(
        state.world.get_block(CELL[0], CELL[1], CELL[2]).unwrap(),
        AIR
    );
    assert_eq!(drop_count(&state), 2);
    assert_eq!(marker(&state).id, id);
    assert_eq!(marker(&state).next_tick, Some(24));
    assert!(
        state
            .entities
            .due_tick_entries(24, None, 16)
            .contains(&(24, id))
    );
    assert!(
        stage(&mut state, 5).unwrap().is_none(),
        "owner deadline must also survive replay"
    );
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn owner_participants_capacity_and_conflicts_reject_every_participant_then_retry() {
    let path = save();
    let mut state = server_state_with_startup(7, path.clone(), 1, startup()).unwrap();
    plant(&mut state);
    let next_id = state.durability.next_id;
    let frontier = state.durability.entity_publication_frontier;
    let mut permits = Vec::new();
    for _ in 0..256 {
        match state
            .durability
            .entity_mirror
            .try_reserve_durable()
            .unwrap()
        {
            Some(permit) => permits.push(permit),
            None => break,
        }
    }
    assert!(
        state
            .durability
            .entity_mirror
            .try_reserve_durable()
            .unwrap()
            .is_none()
    );
    assert_eq!(
        stage(&mut state, 2).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(state.durability.next_id, next_id);
    assert_eq!(state.durability.entity_publication_frontier, frontier);
    assert!(state.durability.reserved.is_empty());
    assert_eq!(owner(&state), (0, vec![0]));
    drop(permits);
    // A pending spawn changes a page the handler observed as empty. The
    // losing owner must not consume its state/deadline or leak a mirror permit.
    let mut action = empty_action();
    action.entities = crate::server::drops::plan_spawn_stack(
        &state.entities,
        state.world.catalog(),
        POSITION,
        crate::inventory::Stack::new(crate::items::STICK, 1),
        Duration::from_millis(250),
        1,
        crate::server::drops::unix_ms(),
    )
    .unwrap();
    assert!(stage_action(&mut state, &action).unwrap());
    let error = stage(&mut state, 3).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::WouldBlock, "{error}");
    assert_eq!(owner(&state), (0, vec![0]));
    assert_eq!(state.durability.pending.len(), 1);
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    let wave = stage(&mut state, 4).unwrap().unwrap();
    // The accepted owner now fences the merge target against a competing take.
    let drop_id = crate::server::drops::nearby(&state.entities, POSITION)[0].id;
    action.entities = crate::server::drops::plan_take(
        &state.entities,
        &[(crate::server::entities::EntityId::new(drop_id).unwrap(), 1)],
    )
    .unwrap();
    assert!(matches!(
        stage_action(&mut state, &action),
        Err(StageError::Conflict)
    ));
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(owner(&state), (1, vec![1]));
    assert_eq!(drop_count(&state), 2);
    assert!(state.durability.reserved.is_empty());
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn owner_participants_failed_receipt_quarantines_all_and_replay_is_authority() {
    let path = save();
    let mut state = server_state_with_startup(7, path.clone(), 1, startup()).unwrap();
    plant(&mut state);
    let wave = stage(&mut state, 2).unwrap().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, receiver);
    real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    sender
        .send(Err(io::Error::other("injected receipt failure")))
        .unwrap();
    assert!(complete_barrier(&mut state, wave.barrier()).is_err());
    assert!(state.durability.failed);
    assert_eq!(owner(&state), (0, vec![0]));
    assert_eq!(drop_count(&state), 0);
    assert_eq!(
        state.world.cached_block(CELL[0], CELL[1], CELL[2]),
        Some(SAND)
    );
    assert!(state.durability.publish_queue.is_empty());
    assert!(!state.durability.reserved.is_empty());
    assert!(poll_journal_receipts(&mut state).is_err());
    drop(state);
    let mut state = server_state_with_startup(7, path.clone(), 1, startup()).unwrap();
    assert_eq!(owner(&state), (1, vec![1]));
    assert_eq!(drop_count(&state), 1);
    assert_eq!(marker(&state).next_tick, Some(52));
    assert_eq!(
        state.world.get_block(CELL[0], CELL[1], CELL[2]).unwrap(),
        AIR
    );
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn owner_participants_chain_disjoint_publication_but_fence_read_only_entities() {
    let path = save();
    let mut state = server_state_with_startup(7, path.clone(), 1, startup()).unwrap();
    plant(&mut state);
    let mut seed = empty_action();
    seed.entities = crate::server::drops::plan_stack_spawns(
        &state.entities,
        state.world.catalog(),
        &[
            (
                [152.5, 100.5, 8.5],
                crate::inventory::Stack::new(crate::items::STICK, 2),
                Duration::ZERO,
            ),
            (
                [200.5, 100.5, 8.5],
                crate::inventory::Stack::new(crate::items::STICK, 2),
                Duration::ZERO,
            ),
        ],
        1,
        crate::server::drops::unix_ms(),
    )
    .unwrap();
    let ids = seed.entities.as_ref().unwrap().entity_ids();
    assert!(stage_action(&mut state, &seed).unwrap());
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    let revision = state.entities.revision();
    let mut disjoint = empty_action();
    disjoint.entities = crate::server::drops::plan_take(&state.entities, &[(ids[1], 1)]).unwrap();
    assert!(stage_action(&mut state, &disjoint).unwrap());
    let wave = stage(&mut state, 2).unwrap().unwrap();
    assert_eq!(state.durability.pending.len(), 2);
    let mut observed = empty_action();
    observed.entities = crate::server::drops::plan_take(&state.entities, &[(ids[0], 1)]).unwrap();
    assert!(matches!(
        stage_action(&mut state, &observed),
        Err(StageError::Conflict)
    ));
    complete_barrier(&mut state, wave.barrier()).unwrap();
    assert_eq!(state.entities.revision(), revision + 2);
    assert!(stage_action(&mut state, &observed).unwrap());
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    assert_eq!(state.entities.revision(), revision + 3);
    assert!(state.durability.reserved.is_empty());
    drop(state);
    let state = server_state_with_startup(7, path.clone(), 1, startup()).unwrap();
    assert_eq!(state.entities.revision(), revision + 3);
    assert_eq!(owner(&state), (1, vec![1]));
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}
