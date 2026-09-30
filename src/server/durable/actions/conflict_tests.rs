//! Exercise real policy capture, WAL admission, ordered receipts and recovery.
use super::*;
use crate::server::durable::{PendingPayload, StageError};
use crate::server::entities::{
    EntityId, EntityPatch, EntityPayload, EntitySpawn, PreparedEntityTransaction,
};
use std::sync::{Arc, mpsc};

const PROBE: crate::content::EntityTypeId = crate::content::EntityTypeId(80);

#[path = "lifecycle_tests.rs"]
mod lifecycle;

struct Probe(bool);
impl crate::server::entities::EntityTickPolicy for Probe {
    fn reads_neighbours(&self) -> bool {
        self.0
    }
    fn read_radius_chunks(&self) -> u8 {
        1
    }
    fn plan(
        &self,
        snapshot: &crate::server::entities::EntitySnapshot,
        _: u64,
        _: &crate::content::Catalog,
        _: &crate::server::voxel_view::VoxelView,
        neighbours: &crate::server::entities::EntityView,
    ) -> Result<crate::server::entities::EntityTickPlan, crate::server::entities::EntityError> {
        Ok(crate::server::entities::EntityTickPlan {
            lifecycle: Default::default(),
            payload: Some(EntityPayload::new(neighbours.len() as u8)),
            next_tick: Some(snapshot.next_tick.unwrap() + 5),
            anchor_update: None,
            block_states: Vec::new(),
            wakes: Vec::new(),
            transfer: None,
            position: None,
        })
    }
}

fn startup(reads: bool) -> crate::server::startup::ServerStartup {
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .register_entity_type(crate::content::EntityTypeDef {
            id: PROBE,
            key: "test:conflict_probe".into(),
            schema_version: 1,
            schema_fingerprint: 880,
        })
        .unwrap();
    let mut startup = crate::server::startup::ServerStartup::new(Arc::new(catalog));
    startup.register_entity_type(crate::server::startup::StartupEntityType {
        key: "test:conflict_probe".into(),
        ownership: crate::server::entities::EntityOwnership::Mobile,
        tick_policy: crate::server::entities::TickPolicy::Interval(5),
        max_payload_bytes: 1,
        codec: Arc::new(CounterCodec),
        interaction_policy: None,
        tick_planner: Some(Arc::new(Probe(reads))),
    });
    startup
}

fn spawn(position: [f32; 3]) -> EntitySpawn {
    EntitySpawn::Mobile {
        entity_type: PROBE,
        position,
        payload: EntityPayload::new(7u8),
        spawn_tick: 1,
    }
}

fn action(entities: PreparedEntityTransaction) -> CommitAction {
    CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        clock_change: None,
        entities: Some(entities),
        entity_wakes: Vec::new(),
        owner_changes: vec![],
        player_publication: None,
    }
}

fn stage(state: &mut State, action: &CommitAction) -> Result<bool, StageError> {
    state
        .entities
        .validate_prepared(action.entities.as_ref().unwrap())
        .unwrap();
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    state
        .durability
        .try_stage(TickId::new(6), action, Some(permit))
}

fn seed(state: &mut State, positions: &[[f32; 3]]) -> Vec<EntityId> {
    let prepared = state
        .entities
        .prepare_spawn_batch(positions.iter().copied().map(spawn).collect())
        .unwrap();
    let ids = prepared.entity_ids();
    assert!(stage(state, &action(prepared)).unwrap());
    crate::server::durable::receipt::drain_staged_receipts(state).unwrap();
    for position in positions {
        let center = world_to_chunk(position[0] as i32, position[1] as i32, position[2] as i32).0;
        for x in center.x - 1..=center.x + 1 {
            for y in center.y - 1..=center.y + 1 {
                for z in center.z - 1..=center.z + 1 {
                    state
                        .world
                        .get_chunk(crate::world::ChunkKey { x, y, z })
                        .unwrap();
                }
            }
        }
    }
    state.durability.publish_queue.clear();
    ids
}

fn plan(state: &mut State, id: EntityId) -> CommitAction {
    super::super::entity::plan_entity_tick(state, id, 6, false)
        .unwrap()
        .unwrap()
}

fn update(state: &State, id: EntityId) -> CommitAction {
    action(
        state
            .entities
            .prepare_update(
                id,
                state.entities.snapshot(id).unwrap().revision,
                EntityPatch {
                    payload: Some(EntityPayload::new(9u8)),
                    ..Default::default()
                },
            )
            .unwrap(),
    )
}

fn hold(
    state: &mut State,
    index: usize,
) -> (
    mpsc::Sender<std::io::Result<crate::server::journal::CommitReceipt>>,
    mpsc::Receiver<std::io::Result<crate::server::journal::CommitReceipt>>,
) {
    let (sender, receiver) = mpsc::channel();
    (
        sender,
        std::mem::replace(&mut state.durability.pending[index].receiver, receiver),
    )
}

#[test]
fn disjoint_updates_admit_before_receipts_including_shared_owner_and_recover_before_apply() {
    for shared_owner in [false, true] {
        for crash in [false, true] {
            let path = temp_save_dir("independent-entity-receipts");
            let mut state =
                crate::server::server_state_with_startup(7, path.clone(), 1, startup(false))
                    .unwrap();
            let positions = [
                [1.5, 100.5, 1.5],
                if shared_owner {
                    [3.5, 100.5, 1.5]
                } else {
                    [64.5, 100.5, 1.5]
                },
            ];
            let ids = seed(&mut state, &positions);
            let before = state.entities.revision();
            let public_before = state.entity_public_revision;
            let first = plan(&mut state, ids[0]);
            let second = plan(&mut state, ids[1]);
            assert!(stage(&mut state, &first).unwrap());
            let (release_first, receipt_first) = hold(&mut state, 0);
            // Both were prepared against the same applied state. The second
            // must stage while the first is committed-but-not-yet-applied.
            assert!(stage(&mut state, &second).unwrap());
            let (release_second, receipt_second) = hold(&mut state, 1);
            assert_eq!(state.durability.pending.len(), 2);
            assert_eq!(state.entities.revision(), before);
            assert_eq!(state.entity_public_revision, public_before);
            assert!(state.durability.publish_queue.is_empty());
            assert!(matches!(
                stage(&mut state, &first),
                Err(StageError::Conflict)
            ));
            assert!(
                !state
                    .durability
                    .reserved
                    .iter()
                    .any(|key| key.domain == crate::server::entities::ENTITY_REVISION_DOMAIN)
            );
            for (offset, pending) in state.durability.pending.iter().enumerate() {
                let PendingPayload::Action(action) = &pending.payload else {
                    panic!("entity action")
                };
                assert_eq!(
                    action
                        .entities
                        .as_ref()
                        .unwrap()
                        .publication_frontier()
                        .unwrap(),
                    (before + offset as u64 + 1, before + offset as u64 + 1)
                );
            }
            let first_receipt = receipt_first
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            let second_receipt = receipt_second
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            if !crash {
                release_second.send(Ok(second_receipt)).unwrap();
                crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
                assert_eq!(
                    state.entities.revision(),
                    before,
                    "later receipt cannot overtake"
                );
                release_first.send(Ok(first_receipt)).unwrap();
                crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
                assert!(state.durability.pending.is_empty());
                assert!(state.durability.reserved.is_empty());
                assert_eq!(state.entities.revision(), before + 2);
                let sequences: Vec<_> = state
                    .durability
                    .publish_queue
                    .iter()
                    .map(|event| event.entity_commit.as_ref().unwrap().registry_revision)
                    .collect();
                assert_eq!(sequences, vec![public_before + 1, public_before + 2]);
                // Fence and serialize the actual ordered checkpoint mirror.
                let mut ticket = state
                    .durability
                    .entity_mirror
                    .try_begin_checkpoint()
                    .unwrap()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                let receipt = loop {
                    if let Some(receipt) = state
                        .durability
                        .entity_mirror
                        .poll_checkpoint(&mut ticket)
                        .unwrap()
                    {
                        break receipt;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "mirror checkpoint completion deadline"
                    );
                    std::thread::yield_now();
                };
                assert_eq!(receipt.durable_sequence, before + 2);
                assert_eq!(receipt.registry_revision, before + 2);
                let bytes = state.durability.entity_store.read().unwrap().unwrap();
                assert_eq!(
                    bytes,
                    crate::server::entities::encode_checkpoint(&state.entities).unwrap()
                );
                state
                    .durability
                    .entity_mirror
                    .finish_checkpoint_fence(ticket)
                    .unwrap();
            }
            drop(state);
            let mut recovered =
                crate::server::server_state_with_startup(7, path.clone(), 1, startup(false))
                    .unwrap();
            assert_eq!(recovered.entities.revision(), before + 2);
            for id in &ids {
                let snapshot = recovered.entities.snapshot(*id).unwrap();
                assert_eq!(snapshot.revision, 2);
                assert_eq!(snapshot.next_tick, Some(11));
                assert_eq!(snapshot.private_payload.downcast_ref::<u8>(), Some(&0));
            }
            // Admission continues from the recovered watermark, not the last
            // applied state in the crashed coordinator.
            let next = update(&recovered, ids[0]);
            assert!(stage(&mut recovered, &next).unwrap());
            crate::server::durable::receipt::drain_staged_receipts(&mut recovered).unwrap();
            assert_eq!(recovered.entities.revision(), before + 3);
            drop(recovered);
            fs::remove_dir_all(path).unwrap();
        }
    }
}

#[test]
fn neighbour_contents_and_empty_membership_pages_fence_pending_writers_in_both_orders() {
    for case in 0..8 {
        let change = case % 4;
        for reader_first in [false, true] {
            let path = temp_save_dir("neighbour-dependencies");
            let mut state =
                crate::server::server_state_with_startup(7, path.clone(), 1, startup(true))
                    .unwrap();
            let ids = seed(
                &mut state,
                &[
                    [1.5, 100.5, 1.5],
                    [3.5, 100.5, 1.5],
                    [64.5, 100.5, 1.5],
                    [96.5, 100.5, 1.5],
                ],
            );
            let input = super::super::entity::capture_tick_input(&mut state, ids[0], 6, false)
                .unwrap()
                .unwrap();
            let mut reader = plan(&mut state, ids[0]);
            if case >= 4 {
                let independent = update(&state, ids[3]);
                reader.entities = Some(
                    state
                        .entities
                        .combine_prepared(vec![
                            reader.entities.take().unwrap(),
                            independent.entities.unwrap(),
                        ])
                        .unwrap(),
                );
            }
            let writer = match change {
                0 => update(&state, ids[1]), // unchanged-size public payload
                1 => action(
                    state
                        .entities
                        .prepare_update(
                            ids[1],
                            1,
                            EntityPatch {
                                position: Some([4.5, 100.5, 1.5]),
                                ..Default::default()
                            },
                        )
                        .unwrap(),
                ),
                2 => action(
                    state
                        .entities
                        .prepare_spawn(spawn([16.5, 100.5, 1.5]))
                        .unwrap(),
                ), // empty page
                3 => action(
                    state
                        .entities
                        .prepare_transfer(ids[2], 1, [16.5, 100.5, 1.5], EntityPatch::default())
                        .unwrap(),
                ),
                _ => unreachable!(),
            };
            let (first, blocked) = if reader_first {
                (&reader, &writer)
            } else {
                (&writer, &reader)
            };
            assert!(stage(&mut state, first).unwrap());
            let (release, receipt) = hold(&mut state, 0);
            if !reader_first && change == 3 {
                let error = super::super::entity::plan_entity_tick(&mut state, ids[2], 6, false)
                    .err()
                    .expect("pending transfer must defer a competing tick");
                assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
            }
            assert!(
                input.is_current(&state),
                "applied snapshot alone cannot see pending writes"
            );
            assert!(matches!(
                stage(&mut state, blocked),
                Err(StageError::Conflict)
            ));
            state
                .entities
                .cancel_prepared(blocked.entities.as_ref().unwrap());
            assert_eq!(state.durability.pending.len(), 1);
            release
                .send(Ok(receipt
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap()))
                .unwrap();
            crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
            assert!(!input.is_current(&state));
            assert!(!state.durability.failed);
            if !reader_first {
                let replanned = plan(&mut state, ids[0]);
                assert!(stage(&mut state, &replanned).unwrap());
                crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
            }
            drop(state);
            fs::remove_dir_all(path).unwrap();
        }
    }
}

#[test]
fn unrelated_commits_keep_capture_valid_but_player_changes_invalidate_neighbour_capture() {
    for reads in [false, true] {
        let path = temp_save_dir("capture-dependency-granularity");
        let mut state =
            crate::server::server_state_with_startup(7, path.clone(), 1, startup(reads)).unwrap();
        let ids = seed(&mut state, &[[1.5, 100.5, 1.5], [64.5, 100.5, 1.5]]);
        let input = super::super::entity::capture_tick_input(&mut state, ids[0], 6, false)
            .unwrap()
            .unwrap();
        let unrelated = update(&state, ids[1]);
        assert!(stage(&mut state, &unrelated).unwrap());
        crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
        assert!(
            input.is_current(&state),
            "unrelated publication is not a conflict"
        );
        state
            .player_entities
            .spawn_session(1, [2.5, 100.5, 1.5])
            .unwrap();
        assert_eq!(
            input.is_current(&state),
            !reads,
            "negative player read is validated"
        );
        drop(state);
        fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn coordinator_admits_two_independent_atomic_pickups_before_applying_either() {
    let path = temp_save_dir("independent-pickups");
    let mut state = server_state(7, path.clone()).unwrap();
    let positions = [[1.5, 100.5, 1.5], [64.5, 100.5, 1.5]];
    let prepared = state
        .entities
        .prepare_spawn_batch(
            positions
                .iter()
                .enumerate()
                .map(|(index, position)| EntitySpawn::Mobile {
                    entity_type: crate::server::drops::DROP_ENTITY_TYPE,
                    position: *position,
                    payload: crate::server::drops::DropEntityPayload::new(
                        crate::inventory::Stack::new(STICK, 3 + index as u16),
                        crate::server::drops::unix_ms(),
                        Duration::ZERO,
                    )
                    .into_entity_payload(),
                    spawn_tick: 1,
                })
                .collect(),
        )
        .unwrap();
    let ids = prepared.entity_ids();
    assert!(stage(&mut state, &action(prepared)).unwrap());
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    let _peer1 = add_test_client(&mut state, positions[0], Inventory::default());
    let first_client = state.clients.remove(&1).unwrap();
    let _peer2 = add_test_client(&mut state, positions[1], Inventory::default());
    let mut second_client = state.clients.remove(&1).unwrap();
    second_client.profile = 18;
    state.clients.insert(1, first_client);
    state.clients.insert(2, second_client);
    state.durability.queued.extend([
        DurableRequest::Pickup { id: 1 },
        DurableRequest::Pickup { id: 2 },
    ]);
    crate::server::durable::coordinator::process_durable_actions(
        &mut state,
        TickId::new(6),
        Instant::now(),
    )
    .unwrap();
    assert_eq!(
        state.durability.pending.len(),
        2,
        "the live coordinator admits both before its next receipt poll"
    );
    for (client, id) in [1, 2].iter().zip(&ids) {
        assert!(
            state.clients[client]
                .inventory
                .slots
                .iter()
                .all(Option::is_none)
        );
        assert!(state.entities.snapshot(*id).is_some());
    }
    let (release_first, receipt_first) = hold(&mut state, 0);
    let (release_second, receipt_second) = hold(&mut state, 1);
    let first = receipt_first
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let second = receipt_second
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    release_second.send(Ok(second)).unwrap();
    crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
    assert_eq!(state.entities.len(), 2);
    release_first.send(Ok(first)).unwrap();
    crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
    assert_eq!(state.entities.len(), 0);
    for (index, client) in [1, 2].iter().enumerate() {
        let stack = state.clients[client].inventory.slots[0].as_ref().unwrap();
        assert_eq!((stack.item, stack.count), (STICK, 3 + index as u16));
    }
    drop(state);
    let recovered = server_state(7, path.clone()).unwrap();
    assert_eq!(recovered.entities.len(), 0);
    for (index, profile) in [17, 18].iter().enumerate() {
        assert_eq!(
            recovered.inventory_store.load(*profile).unwrap().slots[0]
                .as_ref()
                .unwrap()
                .count,
            3 + index as u16
        );
    }
    drop(recovered);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn shared_terrain_reads_stay_reserved_until_the_last_reader_applies() {
    let path = temp_save_dir("shared-terrain-reservation");
    let mut state =
        crate::server::server_state_with_startup(7, path.clone(), 1, startup(false)).unwrap();
    let ids = seed(&mut state, &[[1.5, 100.5, 1.5], [3.5, 100.5, 1.5]]);
    let first = plan(&mut state, ids[0]);
    let second = plan(&mut state, ids[1]);
    assert!(stage(&mut state, &first).unwrap());
    assert!(stage(&mut state, &second).unwrap());
    let (release_first, receipt_first) = hold(&mut state, 0);
    let (release_second, receipt_second) = hold(&mut state, 1);
    let mut terrain = first.clone();
    terrain.entities = None;
    terrain.world_edits = state
        .world
        .prepare_edits(&[(1, 100, 1, crate::world::STONE)])
        .unwrap();
    let key = crate::server::durable::chunk_state_key(world_to_chunk(1, 100, 1).0);
    assert!(state.durability.reserved.contains(&key));
    assert!(matches!(
        state.durability.try_stage(TickId::new(6), &terrain, None),
        Err(StageError::Conflict)
    ));
    release_first
        .send(Ok(receipt_first
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()))
        .unwrap();
    crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
    assert_eq!(state.durability.pending.len(), 1);
    assert!(state.durability.reserved.contains(&key));
    assert!(matches!(
        state.durability.try_stage(TickId::new(6), &terrain, None),
        Err(StageError::Conflict)
    ));
    release_second
        .send(Ok(receipt_second
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()))
        .unwrap();
    crate::server::durable::receipt::poll_journal_receipts(&mut state).unwrap();
    assert!(!state.durability.reserved.contains(&key));
    assert!(
        state
            .durability
            .try_stage(TickId::new(6), &terrain, None)
            .unwrap()
    );
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn drop_merge_absence_is_fenced_against_same_owner_motion_into_range() {
    let path = temp_save_dir("merge-spatial-absence");
    let mut state = server_state(7, path.clone()).unwrap();
    let now = crate::server::drops::unix_ms();
    let make_drop = |state: &State| {
        crate::server::drops::plan_spawn_stack(
            &state.entities,
            state.world.catalog(),
            [1.5, 100.5, 1.5],
            crate::inventory::Stack::new(STICK, 1),
            Duration::ZERO,
            1,
            now,
        )
        .unwrap()
        .unwrap()
    };
    let prepared = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: crate::server::drops::DROP_ENTITY_TYPE,
            position: [4.5, 100.5, 1.5],
            payload: crate::server::drops::DropEntityPayload::new(
                crate::inventory::Stack::new(STICK, 1),
                now,
                Duration::ZERO,
            )
            .into_entity_payload(),
            spawn_tick: 1,
        })
        .unwrap();
    let id = prepared.entity_id();
    assert!(stage(&mut state, &action(prepared)).unwrap());
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    let empty_query = action(make_drop(&state));
    let movement = action(
        state
            .entities
            .prepare_update(
                id,
                1,
                EntityPatch {
                    position: Some([1.75, 100.5, 1.5]),
                    ..Default::default()
                },
            )
            .unwrap(),
    );
    assert!(stage(&mut state, &movement).unwrap());
    assert!(matches!(
        stage(&mut state, &empty_query),
        Err(StageError::Conflict)
    ));
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    let merge = action(make_drop(&state));
    assert!(stage(&mut state, &merge).unwrap());
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    assert_eq!(state.entities.len(), 1);
    assert_eq!(
        crate::server::drops::stack(&state.entities, id)
            .unwrap()
            .count,
        2
    );
    drop(state);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn overlapping_item_transfers_defer_in_the_coordinator_without_partial_ownership() {
    let path = temp_save_dir("pending-item-transfer");
    let source_type = crate::content::EntityTypeId(71_101);
    let sink_type = crate::content::EntityTypeId(71_102);
    let mut state = crate::server::server_state_with_startup(
        7,
        path.clone(),
        1,
        bin_startup(
            bin_catalog(source_type, sink_type),
            source_type,
            sink_type,
            None,
            40,
        ),
    )
    .unwrap();
    state.world.get_chunk(world_to_chunk(0, 80, 0).0).unwrap();
    let prepared = state
        .entities
        .prepare_spawn_batch(
            [
                (source_type, [0.5, 80.0, 0.5], 100),
                (sink_type, [2.5, 80.0, 0.5], 10),
                (sink_type, [4.5, 80.0, 0.5], 10),
            ]
            .into_iter()
            .map(|(entity_type, position, count)| EntitySpawn::Mobile {
                entity_type,
                position,
                payload: EntityPayload::new(BinPayload { item: STICK, count }),
                spawn_tick: 1,
            })
            .collect(),
        )
        .unwrap();
    let ids = prepared.entity_ids();
    assert!(stage(&mut state, &action(prepared)).unwrap());
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    let first = plan(&mut state, ids[1]);
    assert_eq!(first.entities.as_ref().unwrap().entity_ids().len(), 2);
    assert!(stage(&mut state, &first).unwrap());
    let (release, receipt) = hold(&mut state, 0);
    state
        .durability
        .queued
        .push_back(DurableRequest::EntityTick { id: ids[2] });
    crate::server::durable::coordinator::process_durable_actions(
        &mut state,
        TickId::new(6),
        Instant::now(),
    )
    .unwrap();
    assert_eq!(state.durability.pending.len(), 1);
    assert_eq!(state.durability.queued.len(), 1);
    assert_eq!(
        ids.iter()
            .map(|id| bin_count(&state, *id))
            .collect::<Vec<_>>(),
        vec![100, 10, 10]
    );
    release
        .send(Ok(receipt
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()))
        .unwrap();
    crate::server::durable::coordinator::process_durable_actions(
        &mut state,
        TickId::new(6),
        Instant::now(),
    )
    .unwrap();
    assert_eq!(
        ids.iter()
            .map(|id| bin_count(&state, *id))
            .collect::<Vec<_>>(),
        vec![70, 40, 10]
    );
    assert_eq!(state.durability.pending.len(), 1);
    assert!(state.durability.queued.is_empty());
    crate::server::durable::receipt::drain_staged_receipts(&mut state).unwrap();
    assert_eq!(
        ids.iter()
            .map(|id| bin_count(&state, *id))
            .collect::<Vec<_>>(),
        vec![40, 40, 40]
    );
    assert!(!state.durability.failed);
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
