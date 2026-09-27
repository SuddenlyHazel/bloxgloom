use super::*;
use crate::inventory::MAX_COMPONENT_BYTES;
use crate::items::ItemId;

#[test]
fn full_chest_roundtrips_all_slots_and_refuses_overflow_without_losing_components() {
    let catalog = Arc::new(Catalog::builtins());
    let codec = codec::Codec::<27> {
        catalog: catalog.clone(),
        kind: WorkstationKind::Chest,
    };
    let stack = Stack::with_components(
        ItemId(crate::world::STONE.0),
        128,
        1,
        vec![7; MAX_COMPONENT_BYTES],
    )
    .unwrap();
    let payload = EntityPayload::new(StoragePayload::<27> {
        slots: std::array::from_fn(|_| Some(stack.clone())),
    });
    let bytes = codec.encode(&payload).unwrap();
    assert!(bytes.len() <= 27 * 1100 + 128);
    let restored = codec.decode(&bytes).unwrap();
    assert_eq!(
        restored.downcast_ref::<StoragePayload<27>>(),
        payload.downcast_ref::<StoragePayload<27>>()
    );
    let public = crate::protocol::workstation::WorkstationView::decode(
        &codec.public_view(&payload).unwrap(),
    )
    .unwrap();
    assert_eq!(public.slots.len(), 27);
    assert_eq!(public.kind, WorkstationKind::Chest);
    assert!(
        public
            .slots
            .iter()
            .flatten()
            .all(|s| s.count == 128 && s.components.is_none())
    );
    let mut one = stack.clone();
    one.count = 1;
    assert!(
        policy::Port::<StoragePayload<27>>::new()
            .deposit(&payload, &one, &catalog)
            .unwrap()
            .is_none()
    );
    let (after, taken) = policy::Port::<StoragePayload<27>>::new()
        .withdraw(&payload, stack.item, 1, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(taken, one);
    let restored = policy::Port::<StoragePayload<27>>::new()
        .deposit(&after, &taken, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.downcast_ref::<StoragePayload<27>>(),
        payload.downcast_ref::<StoragePayload<27>>()
    );
    // The shared codec still rejects a 27-slot payload as a three-slot Hopper.
    let hopper = codec::Codec::<3> {
        catalog,
        kind: WorkstationKind::Hopper,
    };
    assert!(hopper.decode(&bytes).is_err());
}
