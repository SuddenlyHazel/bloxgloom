//! Real coordinator admission, receipt boundary and WAL restart for both entrypoints.
use super::*;
use crate::inventory::Stack as StoredStack;

struct AnchorExtension;
struct Destroy;
struct Removed;
struct Neighbor;
struct GroundRemoved;

fn withdraw(context: &mut Context<'_>, count: u16, destination: usize) -> Result<(), Error> {
    let id = context.anchored_entity_at([-1, 80, -1])?.unwrap();
    let owner = InventoryId::Entity(id);
    if let Some(player) = context.player() {
        assert!(context.transfer(owner, 8, player, destination, count)?);
    } else {
        let stack = context.take(owner, 8, count)?.unwrap();
        context.spawn_stack([-0.5, 79.5, -0.5], stack, 250)?;
    }
    Ok(())
}

impl Handler for Destroy {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        withdraw(context, 2, 1)?;
        match event {
            Event::ActionRequested { .. } => {
                assert!(context.take(context.player().unwrap(), 2, 1)?.is_some());
            }
            Event::EntityTick { entity, .. } => {
                assert!(context.update_entity(*entity, &[2])?);
                assert!(context.schedule_entity(*entity, None)?);
            }
            _ => unreachable!(),
        }
        // Only the secondary cell is requested; the host must add the anchor.
        context.set_block([-1, 80, -1], "bloxgloom:air")
    }
}

impl Handler for Removed {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        let Event::BlockRemoved { cell, cause, .. } = event else {
            unreachable!()
        };
        assert_eq!(*cell, [-1, 79, -1]);
        assert_eq!(*cause, RemovalCause::AnchoredBreak);
        for y in [79, 80] {
            assert_eq!(context.block([-1, y, -1])?.block_type, "bloxgloom:air");
        }
        // Refunds must use inventory after this callback, not after Destroy.
        withdraw(context, 1, 3)?;
        context.spawn_drop([-0.5, 79.5, -0.5], "bloxgloom:stick", 1, 250)
    }
}

impl Handler for Neighbor {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error> {
        if let Event::NeighborChanged { cell, changed, .. } = event
            && *changed == [-1, 79, -1]
        {
            context.set_block(*cell, "bloxgloom:air")?;
        }
        Ok(())
    }
}

impl Handler for GroundRemoved {
    fn handle(&self, context: &mut Context<'_>, _: &Event) -> Result<(), Error> {
        context.spawn_drop([-0.5, 79.5, -0.5], "bloxgloom:seeds", 1, 250)
    }
}

impl Extension for AnchorExtension {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
        bloxgloom_lifecycle_fixture::TallStore.register(r)?;
        r.action(Action {
            key: "test:destroy_anchor".into(),
            version: 1,
            label: "DESTROY".into(),
            target: Target::Item("bloxgloom:stick".into()),
            operation: Operation::Gameplay,
            panel: None,
            command: None,
        })?;
        r.gameplay_entity(EntityDefinition {
            key: "test:destroyer".into(),
            schema_version: 1,
            schema_fingerprint: 1234,
            max_state_bytes: 1,
            initial_delay_ticks: Some(1),
            state: Arc::new(MarkerState),
        })?;
        for (key, event, target, handler) in [
            (
                "destroy_anchor",
                EventKind::ActionRequested,
                "test:destroy_anchor",
                Arc::new(Destroy) as Arc<dyn Handler>,
            ),
            (
                "destroy_tick",
                EventKind::EntityTick,
                "test:destroyer",
                Arc::new(Destroy),
            ),
            (
                "anchor_removed",
                EventKind::BlockRemoved,
                bloxgloom_lifecycle_fixture::KEY,
                Arc::new(Removed),
            ),
            (
                "anchor_neighbor",
                EventKind::NeighborChanged,
                "bloxgloom:grass",
                Arc::new(Neighbor),
            ),
            (
                "ground_removed",
                EventKind::BlockRemoved,
                "bloxgloom:grass",
                Arc::new(GroundRemoved),
            ),
        ] {
            r.gameplay_handler(HandlerRegistration {
                key: format!("test:{key}"),
                version: 1,
                event,
                target: Some(target.into()),
                handler,
            })?;
        }
        Ok(())
    }
}

fn open_anchor(path: &std::path::Path) -> State {
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&AnchorExtension)
            .unwrap();
    crate::server::server_state_with_startup(53, path.to_owned(), 8, startup).unwrap()
}

fn barrier(state: &mut State) {
    crate::server::durable::complete_barrier(
        state,
        crate::server::durable::CommitBarrier::AllStaged,
    )
    .unwrap();
    assert!(state.durability.pending.is_empty());
}

fn commit_command(state: &mut State, message: ClientMessage, tick: u64) {
    crate::server::durable::coordinator::handle_live_message(state, 1, message, TickId::new(tick))
        .unwrap();
    crate::server::durable::coordinator::process_durable_actions(
        state,
        TickId::new(tick),
        Instant::now(),
    )
    .unwrap();
    assert!(
        state.durability.queued.is_empty(),
        "fixture must have resident inputs"
    );
    barrier(state);
}

#[test]
fn action_and_tick_expand_secondary_cell_once_and_refund_final_inventory_on_restart() {
    for scheduled in [false, true] {
        let path = temp_save_dir(if scheduled {
            "tick-anchor"
        } else {
            "action-anchor"
        });
        let mut state = open_anchor(&path);
        // Tall footprint crosses the vertical seam; neighbor dispatch also reads
        // both horizontal seams. Do not rely on loader sleeps in this harness.
        for x in [-2, -1, 0] {
            for y in [78, 79, 80, 81] {
                for z in [-2, -1, 0, 2] {
                    state.world.get_block(x, y, z).unwrap();
                }
            }
        }
        let catalog = state.world.catalog_arc();
        let block = catalog
            .state_by_key(bloxgloom_lifecycle_fixture::KEY)
            .unwrap();
        let item = catalog.primary_block_item(block).unwrap();
        let tagged =
            StoredStack::with_components(ItemId(crate::world::STONE.0), 7, 1, vec![12, 34])
                .unwrap();
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(StoredStack::new(item, 1));
        inventory.slots[2] = Some(StoredStack::new(STICK, 2));
        inventory.slots[25] = Some(tagged.clone());
        let peer = add_test_client(&mut state, [-1.0, 79.0, 2.0], inventory);
        assert!(
            state
                .durability
                .request_epoch_grant(17, TickId::new(1))
                .unwrap()
                .is_none()
        );
        barrier(&mut state);
        let epoch = u128::from(
            state
                .durability
                .request_epoch_grant(17, TickId::new(2))
                .unwrap()
                .unwrap(),
        ) << 64;
        commit_command(
            &mut state,
            ClientMessage::Edit {
                action_id: epoch | 1,
                x: -1,
                y: 79,
                z: -1,
                block,
                slot: 0,
            },
            10,
        );
        let id = state
            .entities
            .anchored_at(CellCoord::new(-1, 79, -1))
            .unwrap();
        let mut payload = vec![2, 0, 8, 25];
        payload.extend(7u16.to_le_bytes());
        payload.extend(id.get().to_le_bytes());
        payload.extend(state.entities.snapshot(id).unwrap().revision.to_le_bytes());
        commit_command(
            &mut state,
            ClientMessage::EntityInteract {
                action_id: epoch | 2,
                target: [-1, 80, -1],
                payload,
            },
            11,
        );
        let command = ClientMessage::EntityInteract {
            action_id: epoch | 3,
            target: [0; 3],
            payload: Request {
                key: "test:destroy_anchor".into(),
                version: 1,
                slot: 2,
                inventory_revision: state.clients[&1].inventory.revision,
                entity: 0,
                entity_revision: 0,
                arguments: vec![],
            }
            .encode()
            .unwrap(),
        };
        let entities = if scheduled {
            // Seed the general timer through the normal entity WAL participant.
            let entities = state
                .entities
                .prepare_spawn(crate::server::entities::EntitySpawn::Mobile {
                    entity_type: catalog.entity_type_id_by_key("test:destroyer").unwrap(),
                    position: [-0.5, 79.5, 1.5],
                    payload: crate::server::entities::EntityPayload::new(vec![1u8]),
                    spawn_tick: 11,
                })
                .unwrap();
            Some(entities)
        } else {
            None
        };
        let marker = entities.as_ref().map(|entities| entities.entity_id());
        let action = CommitAction {
            client_id: None,
            profile: None,
            action_id: None,
            receipt_value: None,
            receipt_transition: None,
            terrain_reads: TerrainReads::default(),
            inventory_before: None,
            inventory: None,
            world_edits: state
                .world
                .prepare_edits(&[(0, 79, -1, GRASS), (0, 80, -1, RED_FLOWER)])
                .unwrap(),
            deltas: vec![],
            changed_cells: vec![],
            pickups: vec![],
            fire_seed: None,
            clock_change: None,
            weather_change: None,
            entities,
            entity_wakes: vec![],
            owner_changes: vec![],
            player_publication: None,
        };
        let permit = action.entities.as_ref().map(|_| {
            state
                .durability
                .entity_mirror
                .try_reserve_durable()
                .unwrap()
                .unwrap()
        });
        assert!(
            state
                .durability
                .try_stage(TickId::new(11), &action, permit)
                .unwrap()
        );
        barrier(&mut state);
        let request = marker.map_or_else(
            || edit_request(command.clone()),
            |id| DurableRequest::EntityTick { id },
        );
        if !scheduled {
            // Inside the gameplay radius but outside the actor's physical edit
            // reach: even already-staged transfers/refunds must be discarded.
            state.clients.get_mut(&1).unwrap().movement = MovementState::new([-1.0, 79.0, 7.9], 0);
            let outcome = plan_durable_request(&mut state, &request, TickId::new(12));
            assert!(outcome.is_err(), "out-of-reach request was planned");
            assert_eq!(outcome.err().unwrap().kind(), ErrorKind::PermissionDenied,);
            assert_eq!(
                state.clients[&1].inventory.slots[2].as_ref().unwrap().count,
                2
            );
            assert!(state.clients[&1].inventory.slots[1].is_none());
            assert_eq!(
                state
                    .entities
                    .snapshot(id)
                    .unwrap()
                    .private_payload
                    .downcast_ref::<crate::server::entities::container::ContainerPayload>()
                    .unwrap()
                    .slots[8],
                Some(tagged.clone())
            );
            state.clients.get_mut(&1).unwrap().movement = MovementState::new([-1.0, 79.0, 2.0], 0);
        }
        let planned = plan_durable_request(&mut state, &request, TickId::new(12))
            .unwrap()
            .unwrap();
        assert_eq!(planned.changed_cells.len(), 4);
        assert_eq!(
            planned.world_edits.len(),
            4,
            "two vertical chunks plus support across x seam"
        );
        assert!(planned.terrain_reads.entities_current(&state.entities));
        assert_eq!(state.world.cached_block(-1, 79, -1), Some(block));
        assert!(state.entities.snapshot(id).is_some());
        state.durability.queued.push_back(request);
        crate::server::durable::coordinator::process_durable_actions(
            &mut state,
            TickId::new(12),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(
            state.durability.pending.len(),
            1,
            "all effects have one WAL receipt"
        );
        assert!(
            state.entities.snapshot(id).is_some(),
            "admission is not publication"
        );
        barrier(&mut state);
        assert!(!planned.terrain_reads.entities_current(&state.entities));
        if !scheduled {
            commit_command(&mut state, command, 13);
        }
        drop(peer);
        drop(state);
        let mut state = open_anchor(&path);
        for cell in [[-1, 79, -1], [-1, 80, -1], [0, 79, -1], [0, 80, -1]] {
            assert_eq!(
                state.world.get_block(cell[0], cell[1], cell[2]).unwrap(),
                AIR
            );
        }
        assert!(state.entities.snapshot(id).is_none());
        if let Some(marker) = marker {
            assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, None);
            assert_eq!(state.entities.public_view(marker).unwrap().payload, vec![2]);
        }
        let drops = crate::server::drops::nearby(&state.entities, [-0.5, 79.5, -0.5]);
        for (expected, count) in [
            (item, 1),
            (STICK, 1),
            (crate::items::SEEDS, 1),
            (catalog.primary_block_item(RED_FLOWER).unwrap(), 1),
        ] {
            assert_eq!(
                drops
                    .iter()
                    .filter(|drop| drop.item == expected)
                    .map(|drop| drop.count)
                    .sum::<u16>(),
                count
            );
        }
        let dropped = drops
            .iter()
            .filter(|drop| drop.item == tagged.item)
            .map(|drop| {
                let stack = crate::server::drops::stack(
                    &state.entities,
                    crate::server::entities::EntityId::new(drop.id).unwrap(),
                )
                .unwrap();
                assert_eq!(stack.components, tagged.components);
                stack.count
            })
            .sum::<u16>();
        let inventory = state.inventory_store.load(17).unwrap();
        let held = inventory
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.item == tagged.item)
            .map(|stack| {
                assert_eq!(stack.components, tagged.components);
                stack.count
            })
            .sum::<u16>();
        assert_eq!(
            held + dropped,
            7,
            "staged transfers must not duplicate refunds"
        );
        assert_eq!(dropped, if scheduled { 7 } else { 4 });
        drop(state);
        fs::remove_dir_all(path).unwrap();
    }
}
