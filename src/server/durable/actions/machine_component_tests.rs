use super::*;
use crate::server::durable::actions::entity;
use crate::server::entities::{EntityId, EntityPatch};

fn load_neighbours(state: &mut State) {
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
}
fn payload(state: &State, id: EntityId) -> MachinePayload {
    state
        .entities
        .snapshot(id)
        .unwrap()
        .private_payload
        .downcast_ref::<MachinePayload>()
        .unwrap()
        .clone()
}
fn spawn(state: &mut State, y: i32, key: &str, slots: Vec<Option<Stack>>) -> EntityId {
    let catalog = state.world.catalog_arc();
    let block = catalog.state_by_key(key).unwrap();
    let entity_type = catalog.entity_type_id_by_key(key).unwrap();
    state.world.edit(0, y, 0, block).unwrap();
    let anchor = CellCoord::new(0, y, 0);
    stage_entity_spawn(
        state,
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
    )
}
fn settle(state: &mut State, action: &CommitAction, tick: u64) {
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    assert!(
        state
            .durability
            .try_stage(TickId::new(tick), action, Some(permit))
            .unwrap()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !state.durability.pending.is_empty() {
        super::super::super::super::coordinator::process_durable_actions(
            state,
            TickId::new(tick),
            Instant::now(),
        )
        .unwrap();
        assert!(
            Instant::now() < deadline,
            "journal receipt did not complete"
        );
        std::thread::yield_now();
    }
    assert!(!state.durability.failed);
}
fn pulse(state: &mut State, id: EntityId, tick: u64) {
    let action = entity::plan_entity_tick(state, id, tick, false)
        .unwrap()
        .unwrap();
    settle(state, &action, tick);
}

// This behavior uses only the public authoring API. Slot zero is a protected
// template; pulls may fill only slot one with two exact copies of its identity.
struct SelectiveMachine(bloxgloom_host_api::machine::TransferSelection);
impl bloxgloom_host_api::machine::Behavior for SelectiveMachine {
    fn plan(
        &self,
        c: &bloxgloom_host_api::machine::Context<'_>,
    ) -> Result<bloxgloom_host_api::machine::Plan, bloxgloom_host_api::RegistrationError> {
        use bloxgloom_host_api::machine::*;
        assert_eq!(
            c.slots[0].as_ref().unwrap().stack_key,
            c.slots[1].as_ref().unwrap().stack_key
        );
        assert_ne!(
            c.slots[0].as_ref().unwrap().stack_key,
            c.slots[2].as_ref().unwrap().stack_key
        );
        Ok(Plan {
            data: c.data.to_vec(),
            next_tick: c.tick + 20,
            work: vec![Work::Transfer {
                offset: [0, 1, 0],
                own_port: "storage".into(),
                peer_port: Some("storage".into()),
                push: false,
                selection: self.0.clone(),
            }],
        })
    }
}
impl bloxgloom_host_api::Extension for SelectiveMachine {
    fn register(
        &self,
        r: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        use bloxgloom_host_api::{CubeBlock, FootprintCell, InventoryScreen, machine::*};
        let key = "fixture:selector";
        r.cube_block(CubeBlock {
            key: key.into(),
            name: "SELECTOR".into(),
            texture: "bloxgloom:chest_side".into(),
        })?;
        r.inventory_screen(InventoryScreen::storage(
            key,
            key,
            "SELECTOR",
            3,
            3,
            vec![[0; 3]],
        ))?;
        let cells = vec![FootprintCell {
            offset: [0; 3],
            state: key.into(),
        }];
        r.machine(Machine {
            entity: key.into(),
            block: key.into(),
            item: key.into(),
            schema: 1,
            slots: 3,
            interval: 20,
            read_radius: 1,
            reads_neighbours: true,
            variants: vec![Variant {
                placement_state: key.into(),
                idle: cells.clone(),
                active: cells,
            }],
            filters: vec![Filter::any(); 3],
            ports: vec![Port {
                name: "storage".into(),
                faces: FACES.to_vec(),
                insert: vec![1],
                extract: vec![1],
            }],
            process: None,
            behavior: Arc::new(SelectiveMachine(self.0.clone())),
        })
    }
}

#[test]
fn public_exact_selectors_pull_from_storage_without_leaking_components_or_bypassing_ports() {
    use crate::server::entities::container::ContainerPayload;
    use bloxgloom_host_api::machine::{StackSelector, TransferSelection};
    for (source_slot, destination_slot, moves) in [
        (None, 1, true),
        (Some(1), 1, false),
        (Some(2), 1, true),
        (Some(2), 0, false),
    ] {
        let path = temp_save_dir("public-exact-selector");
        let extension = SelectiveMachine(TransferSelection {
            source_slot,
            destination_slot: Some(destination_slot),
            stack: StackSelector::SameAsSlot(0),
            count: 2,
        });
        let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&extension)
            .unwrap();
        let mut state =
            crate::server::server_state_with_startup(55, path.clone(), 8, startup).unwrap();
        load_neighbours(&mut state);
        let item = ItemId(crate::world::STONE.0);
        let red = Stack::with_components(item, 2, 1, vec![1, 2, 3]).unwrap();
        let blue = Stack::with_components(item, 3, 2, vec![1, 2, 3]).unwrap();
        let mut template = blue.clone();
        template.count = 1;
        let mut destination = blue.clone();
        destination.count = 125;
        let target = spawn(
            &mut state,
            80,
            "fixture:selector",
            vec![Some(template.clone()), Some(destination), Some(red.clone())],
        );
        let catalog = state.world.catalog_arc();
        let block = catalog.state_by_key("bloxgloom:chest").unwrap();
        state.world.edit(0, 81, 0, block).unwrap();
        let anchor = CellCoord::new(0, 81, 0);
        let mut slots = vec![None; 27];
        slots[1] = Some(red.clone());
        slots[2] = Some(blue.clone());
        let source = stage_entity_spawn(
            &mut state,
            EntitySpawn::Anchored {
                entity_type: catalog.entity_type_id_by_key("bloxgloom:chest").unwrap(),
                anchor,
                anchor_state: block,
                footprint: vec![anchor],
                payload: EntityPayload::new(ContainerPayload { slots }),
                spawn_tick: 0,
            },
        );
        let input = entity::capture_tick_input(&mut state, target, 20, false)
            .unwrap()
            .unwrap();
        let plan = input.plan().unwrap();
        assert_eq!(plan.transfer.is_some(), moves);
        if let Some(transfer) = &plan.transfer {
            assert_eq!(transfer.count, 2);
            assert_eq!(transfer.route.unwrap().source_slot, 2);
            assert_eq!(transfer.route.unwrap().destination_slot, Some(1));
        }
        let action = entity::commit_tick_plan(&mut state, input, plan)
            .unwrap()
            .unwrap();
        settle(&mut state, &action, 20);
        let receiver = payload(&state, target);
        assert_eq!(receiver.slots[0], Some(template));
        assert_eq!(
            receiver.slots[1].as_ref().unwrap().count,
            if moves { 127 } else { 125 }
        );
        assert_eq!(
            receiver.slots[1].as_ref().unwrap().components,
            blue.components
        );
        let source = state.entities.snapshot(source).unwrap().private_payload;
        let source = source.downcast_ref::<ContainerPayload>().unwrap();
        assert_eq!(source.slots[1], Some(red));
        assert_eq!(
            source.slots[2].as_ref().unwrap().count,
            if moves { 1 } else { 3 }
        );
        assert_eq!(
            source.slots[2].as_ref().unwrap().components,
            blue.components
        );
        drop(state);
        fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn exact_automation_skips_wrong_variant_fences_both_revisions_and_recovers() {
    let path = temp_save_dir("exact-machine-transfer");
    let startup = || ServerStartup::new(Arc::new(crate::content::Catalog::builtins()));
    let mut state =
        crate::server::server_state_with_startup(53, path.clone(), 8, startup()).unwrap();
    load_neighbours(&mut state);
    let item = ItemId(crate::world::STONE.0);
    let red = Stack::with_components(item, 2, 1, vec![1]).unwrap();
    let blue = Stack::with_components(item, 2, 1, vec![2]).unwrap();
    let mut almost_full = blue.clone();
    almost_full.count = 127;
    let sink = spawn(
        &mut state,
        80,
        "bloxgloom:hopper",
        vec![
            Some(almost_full),
            Some(Stack::new(item, 128)),
            Some(Stack::new(item, 128)),
        ],
    );
    let source = spawn(
        &mut state,
        81,
        "bloxgloom:hopper",
        vec![Some(red.clone()), Some(blue.clone()), None],
    );
    let input = entity::capture_tick_input(&mut state, source, 21, false)
        .unwrap()
        .unwrap();
    let plan = input.plan().unwrap();
    assert_eq!(
        plan.transfer.as_ref().unwrap().route.unwrap().source_slot,
        1
    );
    let before = (payload(&state, source), payload(&state, sink));
    let forged_input = entity::capture_tick_input(&mut state, source, 21, false)
        .unwrap()
        .unwrap();
    let mut forged = forged_input.plan().unwrap();
    forged
        .transfer
        .as_mut()
        .unwrap()
        .route
        .as_mut()
        .unwrap()
        .source_slot = 0;
    assert!(
        entity::commit_tick_plan(&mut state, forged_input, forged).is_err(),
        "commit must use the named slot, never silently substitute another same-ID variant"
    );
    assert_eq!((payload(&state, source), payload(&state, sink)), before);
    // A captured slot cannot name a newly installed variant at the same index.
    stage_entity_update(
        &mut state,
        source,
        EntityPatch {
            payload: None,
            next_tick: Some(Some(21)),
            position: None,
        },
        21,
    );
    assert!(entity::commit_tick_plan(&mut state, input, plan).is_err());
    assert_eq!((payload(&state, source), payload(&state, sink)), before);
    let action = entity::plan_entity_tick(&mut state, source, 21, false)
        .unwrap()
        .unwrap();
    assert_eq!(action.entities.as_ref().unwrap().entity_ids().len(), 2);
    // Destination receipt preimage also fences the complete combined batch.
    stage_entity_update(
        &mut state,
        sink,
        EntityPatch {
            payload: None,
            next_tick: Some(Some(40)),
            position: None,
        },
        21,
    );
    assert!(
        state
            .entities
            .validate_prepared(action.entities.as_ref().unwrap())
            .is_err()
    );
    assert_eq!((payload(&state, source), payload(&state, sink)), before);
    pulse(&mut state, source, 21);
    let after_source = payload(&state, source);
    let after_sink = payload(&state, sink);
    assert_eq!(after_source.slots[0], Some(red));
    assert_eq!(after_source.slots[1].as_ref().unwrap().count, 1);
    assert_eq!(
        after_source.slots[1].as_ref().unwrap().components,
        blue.components
    );
    assert_eq!(after_sink.slots[0].as_ref().unwrap().count, 128);
    assert_eq!(
        after_sink.slots[0].as_ref().unwrap().components,
        blue.components
    );
    assert_eq!(
        after_source
            .slots
            .iter()
            .flatten()
            .map(|s| u32::from(s.count))
            .sum::<u32>()
            + after_sink
                .slots
                .iter()
                .flatten()
                .map(|s| u32::from(s.count))
                .sum::<u32>(),
        387
    );
    // A full target is not progress. The ordinary next due tick is retained,
    // without manufacturing space or deleting the blocked source variants.
    pulse(&mut state, source, 41);
    assert_eq!(payload(&state, source).slots, after_source.slots);
    assert_eq!(state.entities.snapshot(source).unwrap().next_tick, Some(61));
    drop(state);
    let restored =
        crate::server::server_state_with_startup(53, path.clone(), 8, startup()).unwrap();
    assert_eq!(payload(&restored, source).slots, after_source.slots);
    assert_eq!(payload(&restored, sink), after_sink);
    drop(restored);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn independent_component_recipes_preserve_progress_and_exact_outputs_across_remap_restart() {
    use bloxgloom_lifecycle_fixture::machine::{KEY, MARKED_INPUT, REFINED_INPUT};
    let mut local = crate::content::Catalog::builtins();
    crate::server::lifecycle::Registration::install(
        &bloxgloom_lifecycle_fixture::Fixture,
        &mut local,
    )
    .unwrap();
    let mut manifest = crate::content::ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        if entry.kind == b'E' && entry.key == KEY {
            entry.id = 700;
        }
        if entry.kind == b'I' && entry.key == "bloxgloom:stone" {
            entry.id = 701;
        }
        if entry.kind == b'I' && entry.key == "bloxgloom:gravel" {
            entry.id = 702;
        }
    }
    manifest.entries.sort_by_key(|e| (e.kind, e.id));
    let mapped = Arc::new(manifest.resolve_catalog(&local).unwrap());
    let path = temp_save_dir("component-process-remapped-recovery");
    let mut state = crate::server::server_state_with_startup(
        54,
        path.clone(),
        8,
        ServerStartup::new(mapped.clone()),
    )
    .unwrap();
    load_neighbours(&mut state);
    let input = Stack::with_components(ItemId(701), 3, 1, MARKED_INPUT.to_vec()).unwrap();
    let id = spawn(
        &mut state,
        80,
        KEY,
        vec![Some(Stack::new(STICK, 1)), Some(input.clone()), None],
    );
    pulse(&mut state, id, 20);
    let in_progress = payload(&state, id);
    assert_eq!(in_progress.progress, 1);
    assert_eq!(in_progress.slots[1], Some(input));
    assert_eq!(in_progress.fuel, 29);
    drop(state);
    // Startup uses ordinary local registration order. content.map restores the
    // saved numeric assignment before adapters resolve their named recipes.
    let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&bloxgloom_lifecycle_fixture::Fixture)
        .unwrap();
    let mut state = crate::server::server_state_with_startup(54, path.clone(), 8, startup).unwrap();
    load_neighbours(&mut state);
    assert_eq!(payload(&state, id), in_progress);
    assert_eq!(state.entities.snapshot(id).unwrap().entity_type.0, 700);
    pulse(&mut state, id, 40);
    pulse(&mut state, id, 60);
    let done = payload(&state, id);
    assert_eq!(done.slots[1].as_ref().unwrap().count, 2);
    let product = Stack::with_components(ItemId(702), 2, 1, MARKED_INPUT.to_vec()).unwrap();
    assert_eq!(done.slots[2], Some(product.clone()));
    // Same item ID but another exact version/payload chooses another recipe.
    let refined = Stack::with_components(ItemId(701), 2, 2, REFINED_INPUT.to_vec()).unwrap();
    let other = spawn(
        &mut state,
        82,
        KEY,
        vec![Some(Stack::new(STICK, 1)), Some(refined), Some(product)],
    );
    pulse(&mut state, other, 20);
    pulse(&mut state, other, 40);
    assert!(payload(&state, other).slots[1].is_none());
    assert_eq!(payload(&state, other).slots[2].as_ref().unwrap().count, 3);
    // A wrong component output with the same item ID does not fit. In
    // particular, no fresh fuel is consumed and no input progress starts.
    let blocked = spawn(
        &mut state,
        84,
        KEY,
        vec![
            Some(Stack::new(STICK, 1)),
            Some(Stack::with_components(ItemId(701), 1, 1, MARKED_INPUT.to_vec()).unwrap()),
            Some(Stack::new(ItemId(702), 1)),
        ],
    );
    let before = payload(&state, blocked);
    pulse(&mut state, blocked, 20);
    let after = payload(&state, blocked);
    assert_eq!(after.slots, before.slots);
    assert_eq!((after.progress, after.fuel), (0, 0));
    drop(state);
    fs::remove_dir_all(path).unwrap();
}
