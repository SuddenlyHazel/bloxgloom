use super::*;
use crate::content::{EntityTypeId, ItemDef, KILN_ITEM, KILN_STATE_COUNT, TextureId};
use crate::inventory::{STACK_LIMIT, Stack};
use crate::items::ItemId;
use crate::server::drops::register_entity_type as register_drop_type;
use crate::server::entities::player::register_player_entity_type;
use crate::server::entities::registry::EntityPayloadCodec;
use crate::server::entities::{EntityStore, EntityTypeRegistry};
use crate::world::ChunkKey;
use std::collections::BTreeSet;

fn registry(catalog: &Arc<Catalog>) -> Arc<EntityTypeRegistry> {
    let mut builder = super::super::registry::EntityTypeRegistryBuilder::new(catalog);
    register_drop_type(&mut builder, catalog.clone()).unwrap();
    register_player_entity_type(&mut builder).unwrap();
    crate::server::entities::mossbun::register(&mut builder, catalog).unwrap();
    register_entity_type(&mut builder, catalog.clone()).unwrap();
    crate::server::entities::hopper::register(&mut builder, catalog).unwrap();
    crate::server::entities::chest::register(&mut builder, catalog).unwrap();
    Arc::new(builder.freeze().unwrap())
}

fn catalog_with_test_output() -> Arc<Catalog> {
    let mut catalog = Catalog::builtins();
    catalog
        .register_item(ItemDef {
            id: ItemId(257),
            key: "test:glass".into(),
            name: "TEST GLASS".into(),
            swatch: [0.7, 0.85, 0.9, 1.0],
            texture: TextureId(3),
            placeable: None,
            sprite: true,
        })
        .unwrap();
    Arc::new(catalog)
}

#[test]
fn compiled_states_cover_both_halves_facings_and_lit_emission() {
    let catalog = Catalog::builtins();
    let mut seen = BTreeSet::new();
    for facing in KilnFacing::ALL {
        for half in [KilnHalf::Lower, KilnHalf::Upper] {
            for lit in [false, true] {
                let state = kiln_state(&catalog, half, facing, lit).unwrap();
                assert!(seen.insert(state));
                let definition = catalog.state(state).unwrap();
                assert_eq!(definition.properties.len(), 3);
                assert_eq!(definition.emission, if lit { 12 } else { 0 });
                assert_eq!(definition.block_type, KILN_BLOCK_TYPE);
                assert_eq!(
                    definition.textures.side,
                    TextureId(if half == KilnHalf::Upper {
                        20
                    } else if lit {
                        22
                    } else {
                        21
                    })
                );
            }
        }
    }
    assert_eq!(seen.len(), KILN_STATE_COUNT as usize);
    assert_eq!(
        kiln_state(&catalog, KilnHalf::Lower, KilnFacing::North, false).unwrap(),
        KILN_DEFAULT_STATE
    );
    let item = catalog.item(KILN_ITEM).unwrap();
    assert_eq!(item.placeable, Some(KILN_DEFAULT_STATE));
    assert_eq!(KilnFacing::from_player_yaw(0.0), Ok(KilnFacing::West));
    assert_eq!(
        KilnFacing::from_player_yaw(std::f32::consts::FRAC_PI_2),
        Ok(KilnFacing::North)
    );
    assert!(KilnFacing::from_player_yaw(f32::NAN).is_err());
    assert_eq!(
        KilnFacing::from_place_state(&catalog, KILN_DEFAULT_STATE),
        Ok(KilnFacing::North)
    );
    assert_eq!(
        KilnFacing::from_place_state(&catalog, BlockStateId(KILN_DEFAULT_STATE.0 + 4)),
        Ok(KilnFacing::East)
    );
    assert!(
        KilnFacing::from_place_state(&catalog, BlockStateId(KILN_DEFAULT_STATE.0 + 1)).is_err()
    );
}

#[test]
fn footprint_and_break_plans_cover_positive_and_negative_chunk_seams() {
    let catalog = Arc::new(Catalog::builtins());
    let mut store = EntityStore::new(registry(&catalog));
    let positive = CellCoord::new(-1, 15, -1);
    let negative = CellCoord::new(0, -1, 0);
    let positive_payload = KilnPayload::new(KilnFacing::East);
    let negative_payload = KilnPayload::new(KilnFacing::West);
    let spawns = vec![
        positive_payload
            .clone()
            .spawn(positive, 1, &catalog)
            .unwrap(),
        negative_payload
            .clone()
            .spawn(negative, 1, &catalog)
            .unwrap(),
    ];
    let batch = store.prepare_spawn_batch(spawns).unwrap();
    let ids = batch.entity_ids();
    store.apply_committed(batch).unwrap();

    let positive_chunks = [
        ChunkKey { x: -1, y: 0, z: -1 },
        ChunkKey { x: -1, y: 1, z: -1 },
    ];
    let negative_chunks = [
        ChunkKey { x: 0, y: -1, z: 0 },
        ChunkKey { x: 0, y: 0, z: 0 },
    ];
    for chunk in positive_chunks.into_iter().chain(negative_chunks) {
        let views = store.public_views_for_chunk(chunk);
        assert_eq!(views.len(), 1);
    }

    for (anchor, payload) in [(positive, positive_payload), (negative, negative_payload)] {
        let [lower, upper] = kiln_footprint(anchor).unwrap().try_into().unwrap();
        let from_lower = plan_break(anchor, lower, &payload, &catalog).unwrap();
        let from_upper = plan_break(anchor, upper, &payload, &catalog).unwrap();
        assert_eq!(from_lower, from_upper);
        assert_eq!(from_lower.removed_cells, vec![lower, upper]);
        assert_eq!(from_lower.drops, vec![Stack::new(KILN_ITEM, 1)]);
    }
    assert_eq!(ids.len(), 2);
}

#[test]
fn codec_projects_workstation_contents_and_recipe_progress_without_components() {
    let catalog = catalog_with_test_output();
    let codec = KilnPayloadCodec {
        catalog: catalog.clone(),
        recipes: Arc::new(
            KilnRecipeBook::new(
                [KilnRecipe {
                    input: ItemId(4),
                    output: Stack::new(ItemId(257), 1),
                    cook_ticks: 20,
                }],
                &catalog,
            )
            .unwrap(),
        ),
    };
    let mut payload = KilnPayload::new(KilnFacing::South);
    payload.slots[INPUT_SLOT_INDEX] = Some(Stack::new(ItemId(4), 4));
    payload.progress_item = Some(ItemId(4));
    payload.slots[FUEL_SLOT_INDEX] = Some(Stack::new(ItemId(9), 2));
    payload.slots[OUTPUT_SLOT_INDEX] = Some(Stack::new(ItemId(257), 3));
    payload.slots[OUTPUT_SLOT_INDEX]
        .as_mut()
        .unwrap()
        .components = Some(Arc::new(
        crate::inventory::ComponentPayload::new(1, vec![0xab; 8]).unwrap(),
    ));
    payload.fuel_remaining = 80;
    payload.lit = true;
    payload.cook_progress = 19;

    let entity_payload = payload.clone().into_entity_payload();
    let private = codec.encode(&entity_payload).unwrap();
    assert!(private.len() < KILN_MAX_PAYLOAD_BYTES);
    let decoded = codec.decode(&private).unwrap();
    assert_eq!(decoded.downcast_ref::<KilnPayload>(), Some(&payload));
    let public = codec.public_view(&decoded).unwrap();
    let summary = crate::protocol::workstation::WorkstationView::decode(&public).unwrap();
    assert_eq!(public.len(), 24);
    assert_eq!(summary.progress, (19 * 255 / 20) as u8);
    assert_eq!(summary.slots[0], payload.slots[0]);
    assert_eq!(summary.slots[2], Some(Stack::new(ItemId(257), 3)));
    assert_eq!(summary.fuel, 80);
}

#[test]
fn workstation_moves_backpack_stacks_atomically_and_rejects_stale_identity() {
    use crate::server::entities::{EntityInteractionPolicy, EntityView};
    let catalog = Arc::new(Catalog::builtins());
    let mut store = EntityStore::new(registry(&catalog));
    let transaction = store
        .prepare_spawn(
            KilnPayload::new(KilnFacing::North)
                .spawn(CellCoord::new(0, 80, 0), 1, &catalog)
                .unwrap(),
        )
        .unwrap();
    let id = transaction.entity_id();
    store.apply_committed(transaction).unwrap();
    let snapshot = store.snapshot(id).unwrap();
    let mut inventory = crate::inventory::Inventory::default();
    inventory.slots[35] = Some(Stack::new(crate::items::STICK, 128));
    let view = crate::server::voxel_view::VoxelView::from_chunks(Vec::<crate::world::Chunk>::new())
        .unwrap();
    let neighbours = EntityView::assemble(Vec::new(), id);
    let mut request = vec![2, 0, 0, 35, 128, 0];
    request.extend(id.get().to_le_bytes());
    request.extend(snapshot.revision.to_le_bytes());
    let result = KilnInteractionPolicy
        .plan(
            &snapshot,
            &request,
            &inventory,
            &catalog,
            &view,
            &neighbours,
        )
        .unwrap();
    assert_eq!(result.inventory.slots[35], None);
    assert_eq!(
        result
            .payload
            .downcast_ref::<KilnPayload>()
            .unwrap()
            .slot(KilnSlot::Fuel)
            .unwrap()
            .count,
        128
    );
    assert_eq!(
        inventory.slots[35].as_ref().unwrap().count,
        128,
        "planning cannot mutate live inventory"
    );
    let mut loaded = snapshot.clone();
    loaded.private_payload = result.payload;
    loaded.revision += 1;
    assert!(
        KilnInteractionPolicy
            .plan(&loaded, &request, &inventory, &catalog, &view, &neighbours)
            .is_err()
    );
    request[1] = 1;
    request[4] = 127;
    request[14..22].copy_from_slice(&loaded.revision.to_le_bytes());
    let taken = KilnInteractionPolicy
        .plan(
            &loaded,
            &request,
            &result.inventory,
            &catalog,
            &view,
            &neighbours,
        )
        .unwrap();
    assert_eq!(taken.inventory.slots[35].as_ref().unwrap().count, 127);
    assert_eq!(
        taken
            .payload
            .downcast_ref::<KilnPayload>()
            .unwrap()
            .slot(KilnSlot::Fuel)
            .unwrap()
            .count,
        1
    );
    request[6..14].copy_from_slice(&(id.get() + 1).to_le_bytes());
    assert!(
        KilnInteractionPolicy
            .plan(
                &loaded,
                &request,
                &result.inventory,
                &catalog,
                &view,
                &neighbours
            )
            .is_err()
    );
    assert!(
        KilnInteractionPolicy
            .plan(
                &loaded,
                &request[..6],
                &result.inventory,
                &catalog,
                &view,
                &neighbours
            )
            .is_err()
    );
}

#[test]
fn interaction_and_tick_plans_conserve_stacks_and_block_on_full_output() {
    let catalog = catalog_with_test_output();
    let recipes = KilnRecipeBook::new(
        [KilnRecipe {
            input: ItemId(4),
            output: Stack::new(ItemId(257), 1),
            cook_ticks: 2,
        }],
        &catalog,
    )
    .unwrap();
    let mut payload = KilnPayload::new(KilnFacing::North);
    let input = plan_insert(
        &payload,
        KilnSlot::Input,
        &Stack::new(ItemId(4), 2),
        &catalog,
    )
    .unwrap();
    assert_eq!(input.remainder, None);
    payload = input.payload;
    payload = plan_insert(
        &payload,
        KilnSlot::Fuel,
        &Stack::new(ItemId(9), 1),
        &catalog,
    )
    .unwrap()
    .payload;

    let first = plan_tick(&payload, &recipes, &catalog, 0).unwrap();
    assert_eq!(first.next_tick, KILN_TICK_INTERVAL);
    payload = first.payload.unwrap();
    assert!(payload.is_lit());
    assert_eq!(payload.cook_progress(), 1);
    let second = plan_tick(&payload, &recipes, &catalog, KILN_TICK_INTERVAL).unwrap();
    payload = second.payload.unwrap();
    assert_eq!(payload.slot(KilnSlot::Input).unwrap().count, 1);
    assert_eq!(
        payload.slot(KilnSlot::Output),
        Some(&Stack::new(ItemId(257), 1))
    );
    assert_eq!(payload.fuel_remaining(), 238);
    assert!(payload.is_lit());

    let take = plan_take(&payload, KilnSlot::Output, 1, &catalog).unwrap();
    assert_eq!(take.taken, Stack::new(ItemId(257), 1));
    assert_eq!(take.payload.slot(KilnSlot::Output), None);
    assert_eq!(take.payload.slot(KilnSlot::Input).unwrap().count, 1);

    let mut blocked = take.payload;
    blocked.slots[OUTPUT_SLOT_INDEX] = Some(Stack::new(ItemId(257), STACK_LIMIT));
    let fuel_before = blocked.slot(KilnSlot::Fuel).cloned();
    let blocked_step = plan_tick(&blocked, &recipes, &catalog, second.next_tick).unwrap();
    assert_eq!(blocked_step.payload.as_ref().unwrap().fuel_remaining(), 237);
    assert!(blocked_step.payload.as_ref().unwrap().is_lit());
    assert_eq!(
        blocked.slot(KilnSlot::Fuel).cloned(),
        fuel_before,
        "blocked output does not start or consume new fuel"
    );
    assert_eq!(blocked.slot(KilnSlot::Input).unwrap().count, 1);
    assert_eq!(
        blocked_step.next_tick,
        second.next_tick + KILN_TICK_INTERVAL
    );
}

#[test]
fn unsupported_input_is_valid_and_burning_fuel_expires_while_idle() {
    let catalog = catalog_with_test_output();
    let no_recipes = KilnRecipeBook::default();
    let mut payload = KilnPayload::new(KilnFacing::North);
    payload = plan_insert(
        &payload,
        KilnSlot::Input,
        &Stack::new(ItemId(4), 1),
        &catalog,
    )
    .unwrap()
    .payload;
    let unsupported =
        plan_tick(&payload, &no_recipes, &catalog, 0).expect("no-recipe state remains codec-valid");
    payload = unsupported.payload.unwrap();
    assert_eq!(payload.cook_progress(), 0);
    assert_eq!(payload.progress_item, None);

    payload.fuel_remaining = 2;
    payload.lit = true;
    let first_idle = plan_tick(&payload, &no_recipes, &catalog, unsupported.next_tick).unwrap();
    let payload = first_idle.payload.unwrap();
    assert_eq!(payload.fuel_remaining(), 1);
    assert!(payload.is_lit());
    let second_idle = plan_tick(&payload, &no_recipes, &catalog, first_idle.next_tick).unwrap();
    let payload = second_idle.payload.unwrap();
    assert_eq!(payload.fuel_remaining(), 0);
    assert!(!payload.is_lit());
    assert_eq!(payload.slot(KilnSlot::Input).unwrap().count, 1);
}

#[test]
fn registry_rejects_duplicate_kiln_registration() {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = super::super::registry::EntityTypeRegistryBuilder::new(&catalog);
    register_entity_type(&mut builder, catalog.clone()).unwrap();
    assert_eq!(
        register_entity_type(&mut builder, catalog.clone()),
        Err(EntityError::DuplicateType(KILN_ENTITY_TYPE))
    );
    assert_eq!(KILN_ENTITY_TYPE, EntityTypeId(3));
}
