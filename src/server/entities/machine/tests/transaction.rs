use super::*;

fn setup() -> (Adapter, MachinePayload, api::Transformation) {
    let catalog = Arc::new(Catalog::builtins());
    let mut definition = (**catalog.machine(KILN_ENTITY_TYPE).unwrap()).clone();
    definition.filters[2] = api::Filter {
        items: vec!["bloxgloom:stone".into()],
        components: false,
    };
    let adapter = Adapter::new(catalog, Arc::new(definition));
    let mut payload = MachinePayload::empty(3, 0);
    payload.slots[1] = Some(Stack::new(ItemId(GRAVEL.0), 4));
    let work = api::Transformation {
        inputs: vec![api::Input {
            slot: 1,
            expected: api::StackValue {
                item: "bloxgloom:gravel".into(),
                count: 4,
                components: None,
            },
            count: 2,
        }],
        outputs: vec![api::Output {
            slot: 2,
            stack: api::StackValue {
                item: "bloxgloom:stone".into(),
                count: 3,
                components: None,
            },
        }],
    };
    (adapter, payload, work)
}

#[test]
fn transformation_is_atomic_on_capacity_and_stale_exact_input() {
    let (adapter, mut payload, work) = setup();
    payload.slots[2] = Some(Stack::new(ItemId(STONE.0), 126));
    let before = payload.clone();
    assert!(!adapter.transform(&mut payload, &work).unwrap());
    assert_eq!(payload, before);
    payload.slots[2] = None;
    payload.slots[1].as_mut().unwrap().count = 5;
    let stale = payload.clone();
    assert!(!adapter.transform(&mut payload, &work).unwrap());
    assert_eq!(payload, stale);
    payload.slots[1].as_mut().unwrap().count = 4;
    assert!(adapter.transform(&mut payload, &work).unwrap());
    assert_eq!(payload.slots[1].as_ref().unwrap().count, 2);
    assert_eq!(payload.slots[2].as_ref().unwrap().count, 3);
    assert!(!adapter.transform(&mut payload, &work).unwrap());
    let bytes = adapter
        .encode(&EntityPayload::new(payload.clone()))
        .unwrap();
    assert_eq!(
        adapter
            .decode(&bytes)
            .unwrap()
            .downcast_ref::<MachinePayload>(),
        Some(&payload)
    );
}

#[test]
fn invalid_transformations_cannot_escape_slots_filters_or_stack_limit() {
    let (adapter, payload, work) = setup();
    let mut variants = Vec::new();
    let mut value = work.clone();
    value.inputs.clear();
    variants.push(value);
    let mut value = work.clone();
    value.inputs.push(value.inputs[0].clone());
    variants.push(value);
    let mut value = work.clone();
    value.outputs.push(value.outputs[0].clone());
    variants.push(value);
    let mut value = work.clone();
    value.outputs[0].slot = 3;
    variants.push(value);
    let mut value = work.clone();
    value.outputs[0].stack.count = 129;
    variants.push(value);
    let mut value = work.clone();
    value.outputs[0].stack.item = "bloxgloom:stick".into();
    variants.push(value);
    let mut value = work.clone();
    value.outputs[0].stack.item = "missing:item".into();
    variants.push(value);
    for value in variants {
        let mut after = payload.clone();
        assert!(adapter.transform(&mut after, &value).is_err());
        assert_eq!(after, payload);
    }
}

#[test]
fn multiple_consumed_slots_and_outputs_are_all_or_nothing() {
    let (adapter, mut payload, mut work) = setup();
    payload.slots[0] = Some(Stack::new(STICK, 2));
    work.inputs.push(api::Input {
        slot: 0,
        expected: api::StackValue {
            item: "bloxgloom:stick".into(),
            count: 2,
            components: None,
        },
        count: 1,
    });
    work.outputs.push(api::Output {
        slot: 0,
        stack: api::StackValue {
            item: "bloxgloom:stick".into(),
            count: 128,
            components: None,
        },
    });
    let before = payload.clone();
    assert!(!adapter.transform(&mut payload, &work).unwrap());
    assert_eq!(
        payload, before,
        "first output and both inputs must roll back"
    );
    work.outputs[1].stack.count = 1;
    assert!(adapter.transform(&mut payload, &work).unwrap());
    assert_eq!(payload.slots[0].as_ref().unwrap().count, 2);
    assert_eq!(payload.slots[1].as_ref().unwrap().count, 2);
    assert_eq!(payload.slots[2].as_ref().unwrap().count, 3);
}

#[test]
fn exact_component_preimages_and_output_schema_are_checked_before_consumption() {
    let (mut adapter, mut payload, mut work) = setup();
    let catalog = Arc::new(
        crate::server::catalog_with_extension(
            Catalog::builtins(),
            &bloxgloom_lifecycle_fixture::content::Content,
        )
        .unwrap(),
    );
    let mut definition = (*adapter.definition).clone();
    let output = bloxgloom_lifecycle_fixture::content::CHIP;
    definition.filters[2] = api::Filter {
        items: vec![output.into()],
        components: true,
    };
    adapter = Adapter::new(catalog, Arc::new(definition));
    payload.slots[1] = Some(Stack::with_components(ItemId(GRAVEL.0), 4, 1, vec![9]).unwrap());
    work.inputs[0].expected.components = Some(api::ComponentValue {
        version: 1,
        bytes: vec![8],
    });
    let before = payload.clone();
    assert!(!adapter.transform(&mut payload, &work).unwrap());
    assert_eq!(payload, before);
    work.inputs[0].expected.components.as_mut().unwrap().bytes = vec![9];
    work.outputs[0].stack.item = output.into();
    work.outputs[0].stack.components = Some(api::ComponentValue {
        version: 1,
        bytes: vec![9],
    });
    assert!(adapter.transform(&mut payload, &work).is_err());
    assert_eq!(payload, before);
}
