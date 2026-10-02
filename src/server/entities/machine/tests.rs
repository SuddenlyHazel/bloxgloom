use super::*;
use crate::{
    content::KILN_ENTITY_TYPE,
    items::{ItemId, STICK},
    world::{GRAVEL, STONE},
};
#[path = "tests/transaction.rs"]
mod transaction;
#[test]
fn registered_process_matches_previous_kiln_burning_and_production_rules() {
    let catalog = Arc::new(Catalog::builtins());
    let adapter = Adapter::new(
        catalog.clone(),
        catalog.machine(KILN_ENTITY_TYPE).unwrap().clone(),
    );
    let mut generic = MachinePayload::empty(3, 0);
    generic.slots[0] = Some(Stack::new(STICK, 2));
    generic.slots[1] = Some(Stack::new(ItemId(GRAVEL.0), 10));
    let mut reference = super::super::KilnPayload::new(super::super::KilnFacing::North);
    for (slot, stack) in [
        (
            super::super::kiln::KilnSlot::Fuel,
            generic.slots[0].as_ref().unwrap(),
        ),
        (
            super::super::kiln::KilnSlot::Input,
            generic.slots[1].as_ref().unwrap(),
        ),
    ] {
        reference = super::super::kiln::plan_insert(&reference, slot, stack, &catalog)
            .unwrap()
            .payload;
    }
    let recipes = super::super::kiln::KilnRecipeBook::builtins(&catalog).unwrap();
    for tick in (20..=2000).step_by(20) {
        adapter.process(&mut generic).unwrap();
        if let Some(next) = super::super::kiln::plan_tick(&reference, &recipes, &catalog, tick)
            .unwrap()
            .payload
        {
            reference = next;
        }
        assert_eq!(generic.fuel, reference.fuel_remaining());
        assert_eq!(generic.progress, reference.cook_progress());
        for (i, slot) in [
            super::super::kiln::KilnSlot::Fuel,
            super::super::kiln::KilnSlot::Input,
            super::super::kiln::KilnSlot::Output,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(generic.slots[i].as_ref(), reference.slot(slot));
        }
        let bytes = adapter
            .encode(&EntityPayload::new(generic.clone()))
            .unwrap();
        assert_eq!(
            adapter
                .decode(&bytes)
                .unwrap()
                .downcast_ref::<MachinePayload>(),
            Some(&generic)
        );
    }
    assert_eq!(generic.slots[2], Some(Stack::new(ItemId(STONE.0), 10)));
}
#[test]
fn full_output_never_consumes_unstarted_fuel_or_input_and_components_survive_codec() {
    let catalog = Arc::new(Catalog::builtins());
    let adapter = Adapter::new(
        catalog.clone(),
        catalog.machine(KILN_ENTITY_TYPE).unwrap().clone(),
    );
    let mut p = MachinePayload::empty(3, 0);
    p.slots[0] = Some(Stack::new(STICK, 1));
    p.slots[1] = Some(Stack::new(ItemId(GRAVEL.0), 2));
    p.slots[2] = Some(Stack::new(ItemId(STONE.0), 128));
    let slots = p.slots.clone();
    adapter.process(&mut p).unwrap();
    assert_eq!(p.slots, slots);
    assert_eq!((p.fuel, p.progress), (0, 0));
    let tagged = Stack::with_components(ItemId(GRAVEL.0), 2, 1, vec![5, 9]).unwrap();
    p.slots[1] = Some(tagged.clone());
    p.progress_item = None;
    let encoded = adapter.encode(&EntityPayload::new(p)).unwrap();
    let restored = adapter.decode(&encoded).unwrap();
    assert_eq!(
        restored.downcast_ref::<MachinePayload>().unwrap().slots[1],
        Some(tagged)
    );
}

#[test]
fn component_fuels_and_present_predicates_use_exact_stacks_and_reset_replacement_progress() {
    use api::{ComponentMatch, ComponentOutput, ComponentValue, Fuel};
    let catalog = Arc::new(Catalog::builtins());
    let mut definition = (**catalog.machine(KILN_ENTITY_TYPE).unwrap()).clone();
    definition.filters[0].components = true;
    definition.ports[0].extract.push(1);
    let process = definition.process.as_mut().unwrap();
    process.recipes[0].input_components = ComponentMatch::Present;
    process.recipes[0].output_components = ComponentOutput::PreserveInput;
    process.fuels.push(Fuel {
        item: "bloxgloom:stick".into(),
        pulses: 2,
        components: ComponentMatch::Exact(ComponentValue {
            version: 3,
            bytes: vec![7],
        }),
    });
    definition.validate().unwrap();
    let adapter = Adapter::new(catalog.clone(), Arc::new(definition));
    let mut p = MachinePayload::empty(3, 0);
    let first = Stack::with_components(ItemId(GRAVEL.0), 1, 12, vec![9, 5]).unwrap();
    p.slots[1] = Some(first.clone());
    p.slots[0] = Some(Stack::with_components(STICK, 2, 2, vec![7]).unwrap());
    adapter.process(&mut p).unwrap();
    assert_eq!((p.progress, p.fuel), (0, 0));
    assert_eq!(p.slots[0].as_ref().unwrap().count, 2);
    p.slots[0] = Some(Stack::with_components(STICK, 2, 3, vec![7]).unwrap());
    adapter.process(&mut p).unwrap();
    assert_eq!((p.progress, p.fuel), (1, 1));
    let port = EntityTransferPolicy::port(&adapter, 0, [0, 1, 0]).unwrap();
    let (taken, exact) = port
        .at_slot(1)
        .unwrap()
        .withdraw(&EntityPayload::new(p), first.item, 1, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(exact, first);
    assert_eq!(taken.downcast_ref::<MachinePayload>().unwrap().progress, 0);
    let replacement = Stack::with_components(first.item, 1, 12, vec![9, 6]).unwrap();
    let deposited = port
        .deposit(&taken, &replacement, &catalog)
        .unwrap()
        .unwrap();
    let mut p = deposited.downcast_ref::<MachinePayload>().unwrap().clone();
    assert_eq!(p.progress, 0);
    for _ in 0..3 {
        adapter.process(&mut p).unwrap();
    }
    assert!(
        p.slots[2].is_none(),
        "replacement must earn all four pulses"
    );
    // Fuel is now exhausted; fresh matching fuel is required, not a free pulse.
    assert_eq!(p.fuel, 0);
    p.slots[0] = Some(Stack::with_components(STICK, 1, 3, vec![7]).unwrap());
    adapter.process(&mut p).unwrap();
    assert!(p.slots[1].is_none());
    let output = p.slots[2].as_ref().unwrap();
    assert_eq!(output.item, ItemId(STONE.0));
    assert_eq!(output.components, replacement.components);
    let encoded = adapter.encode(&EntityPayload::new(p.clone())).unwrap();
    assert_eq!(
        adapter
            .decode(&encoded)
            .unwrap()
            .downcast_ref::<MachinePayload>(),
        Some(&p)
    );
}

#[test]
fn ambiguous_or_oversized_component_operations_are_rejected_and_metadata_is_identity() {
    use api::{ComponentMatch, ComponentOutput, ComponentValue};
    let catalog = Catalog::builtins();
    let mut definition = (**catalog.machine(KILN_ENTITY_TYPE).unwrap()).clone();
    let original = definition.fingerprint_bytes();
    let mut variant = definition.process.as_ref().unwrap().recipes[0].clone();
    variant.key = "fixture:variant".into();
    variant.input_components = ComponentMatch::Exact(ComponentValue {
        version: 1,
        bytes: vec![1],
    });
    variant.output_components = ComponentOutput::PreserveInput;
    definition
        .process
        .as_mut()
        .unwrap()
        .recipes
        .push(variant.clone());
    definition.validate().unwrap();
    assert_ne!(definition.fingerprint_bytes(), original);
    let first = definition.fingerprint_bytes();
    variant.input_components = ComponentMatch::Exact(ComponentValue {
        version: 2,
        bytes: vec![1],
    });
    definition.process.as_mut().unwrap().recipes[1] = variant.clone();
    assert_ne!(definition.fingerprint_bytes(), first);
    variant.key = "fixture:ambiguous".into();
    variant.input_components = ComponentMatch::Present;
    definition.process.as_mut().unwrap().recipes.push(variant);
    assert!(definition.validate().is_err());
    definition.process.as_mut().unwrap().recipes.pop();
    definition.process.as_mut().unwrap().recipes[1].output_components =
        ComponentOutput::Exact(ComponentValue {
            version: 1,
            bytes: vec![0; 1025],
        });
    assert!(definition.validate().is_err());
}

#[test]
fn preserved_components_incompatible_with_output_schema_do_not_consume_input_or_fuel() {
    let catalog = Arc::new(
        crate::server::catalog_with_extension(
            Catalog::builtins(),
            &bloxgloom_lifecycle_fixture::content::Content,
        )
        .unwrap(),
    );
    let mut definition = (**catalog.machine(KILN_ENTITY_TYPE).unwrap()).clone();
    let output = bloxgloom_lifecycle_fixture::content::CHIP;
    definition.filters[2].items = vec![output.into()];
    definition.filters[2].components = true;
    let recipe = &mut definition.process.as_mut().unwrap().recipes[0];
    recipe.input_components = api::ComponentMatch::Present;
    recipe.output_components = api::ComponentOutput::PreserveInput;
    recipe.output = output.into();
    let adapter = Adapter::new(catalog, Arc::new(definition));
    let mut payload = MachinePayload::empty(3, 0);
    payload.slots[0] = Some(Stack::new(STICK, 2));
    payload.slots[1] = Some(Stack::with_components(ItemId(GRAVEL.0), 2, 1, vec![7]).unwrap());
    let slots = payload.slots.clone();
    adapter.process(&mut payload).unwrap();
    assert_eq!(payload.slots, slots);
    assert_eq!((payload.progress, payload.fuel), (0, 0));
}
