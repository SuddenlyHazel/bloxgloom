use super::*;
fn action(key: &str) -> Action {
    Action {
        key: key.into(),
        version: 1,
        label: "WORK".into(),
        target: Target::Item("test:input".into()),
        operation: Operation::Recipe {
            input: "test:input".into(),
            consume: 1,
            output: "test:output".into(),
            produce: 1,
        },
        panel: None,
    }
}
#[test]
fn discovery_is_bounded_ordered_and_rejects_conflicting_ownership() {
    let mut registry = Registry::default();
    for i in (0..MAX_TARGET_ACTIONS).rev() {
        registry.register(action(&format!("test:a{i}"))).unwrap();
    }
    assert!(registry.register(action("test:a0")).is_err());
    assert!(registry.register(action("test:overflow")).is_err());
    assert_eq!(
        registry
            .discover(&Target::Item("test:input".into()))
            .map(|a| a.key.as_str())
            .collect::<Vec<_>>(),
        (0..MAX_TARGET_ACTIONS)
            .map(|i| format!("test:a{i}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(registry.discover(&Target::Empty).count(), 0);
}
#[test]
fn request_codec_rejects_truncation_trailing_and_oversized_arguments() {
    let request = Request {
        key: "test:work".into(),
        version: 1,
        slot: 3,
        inventory_revision: 9,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    let bytes = request.encode().unwrap();
    assert_eq!(Request::decode(&bytes), Some(request));
    for n in 0..bytes.len() {
        assert!(Request::decode(&bytes[..n]).is_none());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(Request::decode(&extra).is_none());
    let mut extra = bytes;
    extra.extend([0; 256]);
    assert!(Request::decode(&extra).is_none());
}

#[test]
fn request_codec_preserves_short_argument_encoding() {
    let mut request = Request {
        key: "test:work".into(),
        version: 1,
        slot: 3,
        inventory_revision: 9,
        entity: 0,
        entity_revision: 0,
        arguments: vec![],
    };
    let mut expected = vec![REQUEST_TAG, 9];
    expected.extend(b"test:work");
    expected.extend(1_u16.to_le_bytes());
    expected.push(3);
    expected.extend(9_u64.to_le_bytes());
    expected.extend(0_u64.to_le_bytes());
    expected.extend(0_u64.to_le_bytes());
    for count in 0..=4 {
        request.arguments = (0..count).collect();
        let mut encoded = expected.clone();
        encoded.push(count as u8);
        encoded.extend(&request.arguments);
        assert_eq!(request.encode(), Some(encoded.clone()));
        assert_eq!(Request::decode(&encoded), Some(request.clone()));
    }
}

#[test]
fn request_codec_bounds_long_arguments_and_rejects_noncanonical_lengths() {
    let content_key = format!("test:{}", "x".repeat(123)); // 128-byte content key
    let mut request = Request {
        key: format!("test:{}", "a".repeat(91)), // 96-byte action key
        version: 1,
        slot: 0,
        inventory_revision: 0,
        entity: 0,
        entity_revision: 0,
        arguments: [2_u16.to_le_bytes().as_slice(), content_key.as_bytes()].concat(),
    };
    let bytes = request.encode().unwrap();
    assert_eq!(bytes.len(), 256);
    assert_eq!(Request::decode(&bytes), Some(request.clone()));
    for n in 0..bytes.len() {
        assert!(Request::decode(&bytes[..n]).is_none());
    }
    let length_offset = 29 + request.key.len();
    for length in [0, 129, 131, 255] {
        let mut malformed = bytes.clone();
        malformed[length_offset] = length;
        assert!(Request::decode(&malformed).is_none());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(Request::decode(&trailing).is_none());
    request.arguments.push(0);
    assert!(request.encode().is_none());
    request.arguments.pop();
    request.key.push('a');
    assert!(request.encode().is_none());
    request.key = "test:spawn".into();
    request.arguments = content_key.into_bytes();
    let spawn = request.encode().unwrap();
    assert_eq!(Request::decode(&spawn), Some(request));
}

#[test]
fn terrain_request_distinguishes_zero_from_missing_and_bounds_nested_payload() {
    let observed = TerrainRequest {
        version: 0,
        request: Request {
            key: "test:work".into(),
            version: 1,
            slot: 0,
            inventory_revision: 0,
            entity: 0,
            entity_revision: 0,
            arguments: vec![],
        },
    };
    let bytes = observed.encode().unwrap();
    assert_eq!(TerrainRequest::decode(&bytes), Some(observed.clone()));
    assert!(Request::decode(&bytes).is_none());
    assert!(TerrainRequest::decode(&observed.request.encode().unwrap()).is_none());
    for n in 0..bytes.len() {
        assert!(TerrainRequest::decode(&bytes[..n]).is_none());
    }
    let mut extra = bytes;
    extra.push(0);
    assert!(TerrainRequest::decode(&extra).is_none());
    extra.extend([0; 256]);
    assert!(TerrainRequest::decode(&extra).is_none());
    let mut largest = observed;
    largest.version = u64::MAX;
    largest.request.key = format!("test:{}", "x".repeat(123));
    largest.request.arguments = vec![0; 4];
    let bytes = largest.encode().unwrap();
    assert_eq!(bytes.len(), 171);
    assert_eq!(TerrainRequest::decode(&bytes), Some(largest));
}
#[test]
fn terrain_request_bounds_long_arguments_including_nested_request() {
    let mut observed = TerrainRequest {
        version: 1,
        request: Request {
            key: format!("test:{}", "a".repeat(82)), // 87-byte action key
            version: 1,
            slot: 0,
            inventory_revision: 0,
            entity: 0,
            entity_revision: 0,
            arguments: vec![b'x'; 130],
        },
    };
    let bytes = observed.encode().unwrap();
    assert_eq!(bytes.len(), 256);
    assert_eq!(TerrainRequest::decode(&bytes), Some(observed.clone()));
    for n in 0..bytes.len() {
        assert!(TerrainRequest::decode(&bytes[..n]).is_none());
    }
    let mut malformed = bytes.clone();
    malformed[9 + 29 + observed.request.key.len()] = 129;
    assert!(TerrainRequest::decode(&malformed).is_none());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(TerrainRequest::decode(&trailing).is_none());
    observed.request.key.push('a');
    assert!(observed.request.encode().is_some());
    assert!(observed.encode().is_none());
}
#[test]
fn composition_bounds_and_fingerprint_cover_every_control() {
    let mut definition = action("test:work");
    definition.panel = Some(Panel {
        title: "WORK".into(),
        widgets: vec![Widget::Label("plain text".into()); MAX_WIDGETS],
    });
    definition.validate().unwrap();
    let before = definition.fingerprint_bytes();
    definition.panel.as_mut().unwrap().widgets[0] = Widget::Button {
        action: None,
        label: "WORK".into(),
        tooltip: "Costs one item".into(),
    };
    assert_ne!(before, definition.fingerprint_bytes());
    definition
        .panel
        .as_mut()
        .unwrap()
        .widgets
        .push(Widget::Label("overflow".into()));
    assert!(definition.validate().is_err());
}

#[test]
fn composed_controls_resolve_forward_references_without_changing_target_context() {
    let mut registry = Registry::default();
    let mut first = action("test:first");
    first.panel = Some(Panel {
        title: "TOOLS".into(),
        widgets: vec![Widget::Button {
            label: "SECOND".into(),
            tooltip: "Invoke the other registered action".into(),
            action: Some("test:second".into()),
        }],
    });
    registry.register(first).unwrap();
    assert!(registry.validate_composition().is_err());
    registry.register(action("test:second")).unwrap();
    registry.validate_composition().unwrap();
    let mut bad = action("test:bad");
    bad.target = Target::Empty;
    bad.panel = Some(Panel {
        title: "WRONG TARGET".into(),
        widgets: vec![Widget::Button {
            label: "SECOND".into(),
            tooltip: "Cannot change actor targeting".into(),
            action: Some("test:second".into()),
        }],
    });
    registry.register(bad).unwrap();
    assert!(registry.validate_composition().is_err());
}
