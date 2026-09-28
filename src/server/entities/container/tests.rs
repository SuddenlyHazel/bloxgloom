use super::*;

#[test]
fn storage_face_allowlist_denies_sides_but_keeps_top_slot_permissions_and_exact_slots() {
    let catalog = Arc::new(Catalog::builtins());
    let mut screen = bloxgloom_host_api::InventoryScreen::storage(
        "test:store",
        "test:store",
        "STORE",
        2,
        2,
        vec![[0; 3]],
    );
    screen.groups = vec![
        bloxgloom_host_api::SlotGroup {
            label: "INPUT".into(),
            first: 0,
            count: 1,
            insert: true,
            extract: false,
        },
        bloxgloom_host_api::SlotGroup {
            label: "OUTPUT".into(),
            first: 1,
            count: 1,
            insert: false,
            extract: true,
        },
    ];
    screen.validate().unwrap();
    let mut declaration = super::super::chest::definition();
    declaration.automation_faces = Some(vec![[0, 1, 0]]);
    declaration.validate().unwrap();
    let port = policy::Port::<ContainerPayload>::for_screen_with_faces(
        Arc::new(screen),
        declaration.allowed_automation_faces(),
    );
    assert_eq!(port.ports(), vec!["storage"]);
    for face in [[1, 0, 0], [-1, 0, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
        assert!(port.port(0, face).is_none());
    }
    assert!(port.port(1, [0, 1, 0]).is_none());
    let top = port.port(0, [0, 1, 0]).unwrap();
    let codec = Codec {
        catalog: catalog.clone(),
        slots: 2,
    };
    let stack = Stack::with_components(crate::items::STICK, 7, 1, vec![3, 5, 8]).unwrap();
    let original = EntityPayload::new(ContainerPayload {
        slots: vec![None, Some(stack.clone())],
    });
    let bytes = codec.encode(&original).unwrap();
    let restored = codec.decode(&bytes).unwrap();
    assert_eq!(codec.encode(&restored).unwrap(), bytes);
    // The public inventory projection intentionally omits private components;
    // the authoritative withdrawal below must preserve them exactly.
    let offers = top.offers(&codec.public_view(&restored).unwrap());
    assert_eq!(offers.len(), 1);
    assert_eq!((offers[0].item, offers[0].count), (stack.item, stack.count));
    assert!(
        top.at_slot(0)
            .unwrap()
            .withdraw(&restored, stack.item, 1, &catalog)
            .unwrap()
            .is_none()
    );
    assert!(
        top.at_slot(1)
            .unwrap()
            .deposit(&restored, &stack, &catalog)
            .unwrap()
            .is_none()
    );
    let (after, taken) = top
        .withdraw(&restored, stack.item, 1, &catalog)
        .unwrap()
        .unwrap();
    assert_eq!(taken.count, 1);
    assert_eq!(taken.components, stack.components);
    let after = top.deposit(&after, &taken, &catalog).unwrap().unwrap();
    let mut remaining = stack;
    remaining.count -= 1;
    let expected = ContainerPayload {
        slots: vec![Some(taken), Some(remaining)],
    };
    assert_eq!(after.downcast_ref::<ContainerPayload>(), Some(&expected));
    let bytes = codec.encode(&after).unwrap();
    assert_eq!(
        codec
            .decode(&bytes)
            .unwrap()
            .downcast_ref::<ContainerPayload>(),
        Some(&expected)
    );
}

#[test]
fn registered_storage_automation_respects_slot_access_and_agrees_with_discovery() {
    let catalog = Arc::new(Catalog::builtins());
    let mut screen = bloxgloom_host_api::InventoryScreen::storage(
        "test:store",
        "test:store",
        "STORE",
        2,
        2,
        vec![[0; 3]],
    );
    screen.groups = vec![
        bloxgloom_host_api::SlotGroup {
            label: "INPUT".into(),
            first: 0,
            count: 1,
            insert: true,
            extract: false,
        },
        bloxgloom_host_api::SlotGroup {
            label: "OUTPUT".into(),
            first: 1,
            count: 1,
            insert: false,
            extract: true,
        },
    ];
    screen.validate().unwrap();
    let port = policy::Port::<ContainerPayload>::for_screen(Arc::new(screen));
    assert!(port.port(0, [1, 0, 0]).is_some());
    let codec = Codec {
        catalog: catalog.clone(),
        slots: 2,
    };
    let payload = EntityPayload::new(ContainerPayload {
        slots: vec![Some(Stack::new(crate::items::STICK, 128)), None],
    });
    let public = codec.public_view(&payload).unwrap();
    let one = Stack::new(crate::items::STICK, 1);
    assert!(port.offers(&public).is_empty());
    assert!(!port.accepts(&public, &one, &catalog));
    assert!(
        port.withdraw(&payload, one.item, 1, &catalog)
            .unwrap()
            .is_none()
    );
    assert!(port.deposit(&payload, &one, &catalog).unwrap().is_none());
}

#[test]
fn registered_and_fixed_storage_share_format_and_preserve_exact_components() {
    let catalog = Arc::new(Catalog::builtins());
    let old = super::super::storage::codec::Codec::<27> {
        catalog: catalog.clone(),
    };
    let codec = Codec {
        catalog: catalog.clone(),
        slots: 27,
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
    let small = Codec { catalog, slots: 9 };
    assert!(small.decode(&bytes).is_err());
}

#[test]
fn runtime_capacity_projection_is_discoverable_by_shared_inventory_ports() {
    let catalog = Arc::new(Catalog::builtins());
    let codec = Codec {
        catalog: catalog.clone(),
        slots: 9,
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
