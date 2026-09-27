use super::*;

#[test]
fn registered_chest_preserves_existing_private_and_public_bytes_and_exact_components() {
    let catalog = Arc::new(Catalog::builtins());
    let old = super::super::storage::codec::Codec::<27> {
        catalog: catalog.clone(),
        kind: WorkstationKind::Chest,
    };
    let codec = Codec {
        catalog: catalog.clone(),
        slots: 27,
        legacy_chest_view: true,
    };
    let stack = Stack::with_components(
        crate::items::STICK,
        128,
        1,
        vec![9; crate::inventory::MAX_COMPONENT_BYTES],
    )
    .unwrap();
    let legacy = EntityPayload::new(super::super::storage::StoragePayload::<27> {
        slots: std::array::from_fn(|_| Some(stack.clone())),
    });
    let bytes = old.encode(&legacy).unwrap();
    let payload = codec.decode(&bytes).unwrap();
    assert_eq!(codec.encode(&payload).unwrap(), bytes);
    assert_eq!(
        codec.public_view(&payload).unwrap(),
        old.public_view(&legacy).unwrap()
    );
    assert_eq!(
        payload.downcast_ref::<ContainerPayload>().unwrap().slots,
        vec![Some(stack.clone()); 27]
    );
    let mut one = stack;
    one.count = 1;
    assert!(
        policy::Port::<ContainerPayload>::new()
            .deposit(&payload, &one, &catalog)
            .unwrap()
            .is_none()
    );
    let (after, taken) = policy::Port::<ContainerPayload>::new()
        .withdraw(&payload, one.item, 1, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(taken, one);
    let restored = policy::Port::<ContainerPayload>::new()
        .deposit(&after, &taken, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(codec.encode(&restored).unwrap(), bytes);
    let small = Codec {
        catalog,
        slots: 9,
        legacy_chest_view: false,
    };
    assert!(small.decode(&bytes).is_err());
}

#[test]
fn runtime_capacity_projection_is_discoverable_by_shared_inventory_ports() {
    let catalog = Arc::new(Catalog::builtins());
    let codec = Codec {
        catalog: catalog.clone(),
        slots: 9,
        legacy_chest_view: false,
    };
    let mut payload = ContainerPayload {
        slots: vec![None; 9],
    };
    payload.slots[8] = Some(Stack::new(crate::items::STICK, 17));
    let payload = EntityPayload::new(payload);
    let public = codec.public_view(&payload).unwrap();
    let port = policy::Port::<ContainerPayload>::new();
    assert_eq!(
        port.offers(&public),
        vec![Stack::new(crate::items::STICK, 17)]
    );
    assert!(port.accepts(&public, &Stack::new(crate::items::STICK, 1), &catalog));
    assert!(port.offers(&public[..public.len() - 1]).is_empty());
    let decoded = codec.decode(&codec.encode(&payload).unwrap()).unwrap();
    assert_eq!(
        decoded.downcast_ref::<ContainerPayload>(),
        payload.downcast_ref::<ContainerPayload>()
    );
}
