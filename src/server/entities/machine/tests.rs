use super::*;
use crate::{
    content::KILN_ENTITY_TYPE,
    items::{ItemId, STICK},
    world::{GRAVEL, STONE},
};
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
