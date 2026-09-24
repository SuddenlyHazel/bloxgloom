use super::*;

#[test]
fn drop_payload_round_trips_versioned_stack_data_but_projects_only_public_stack() {
    let catalog = Arc::new(Catalog::builtins());
    let codec = DropPayloadCodec {
        catalog: Arc::clone(&catalog),
    };
    let item = ItemId::new(1);
    let stack = Stack::with_components(item, 127, 3, vec![4, 5, 6]).unwrap();
    let payload = DropEntityPayload::new(stack, 1_234_567, Duration::from_millis(750));
    let payload = EntityPayload::new(payload);
    let bytes = codec.encode(&payload).unwrap();
    let decoded = codec.decode(&bytes).unwrap();
    assert_eq!(codec.encode(&decoded).unwrap(), bytes);
    assert_eq!(
        decoded.downcast_ref::<DropEntityPayload>(),
        payload.downcast_ref::<DropEntityPayload>()
    );
    assert_eq!(codec.public_view(&decoded).unwrap(), [1, 0, 0, 0, 127, 0]);
    assert_eq!(bytes.len(), DROP_PAYLOAD_FIXED_BYTES + 3);
}

#[test]
fn drop_payload_codec_fails_closed_on_invalid_items_counts_and_component_frames() {
    let catalog = Arc::new(Catalog::builtins());
    let codec = DropPayloadCodec { catalog };
    let item = ItemId::new(1);
    let mut unknown_item = codec
        .encode(&EntityPayload::new(DropEntityPayload::new(
            Stack::new(item, 1),
            0,
            Duration::ZERO,
        )))
        .unwrap();
    unknown_item[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        codec.decode(&unknown_item),
        Err(EntityCodecError::InvalidData)
    ));

    let mut invalid_count = codec
        .encode(&EntityPayload::new(DropEntityPayload::new(
            Stack::new(item, 1),
            0,
            Duration::ZERO,
        )))
        .unwrap();
    invalid_count[4..6].copy_from_slice(&129u16.to_le_bytes());
    assert!(matches!(
        codec.decode(&invalid_count),
        Err(EntityCodecError::InvalidData)
    ));

    let mut bad_component_header = codec
        .encode(&EntityPayload::new(DropEntityPayload::new(
            Stack::new(item, 1),
            0,
            Duration::ZERO,
        )))
        .unwrap();
    bad_component_header[22..24].copy_from_slice(&1u16.to_le_bytes());
    assert!(matches!(
        codec.decode(&bad_component_header),
        Err(EntityCodecError::InvalidData)
    ));
}

#[test]
fn drop_entity_registration_is_catalog_linked_and_mobile() {
    let catalog = Arc::new(Catalog::builtins());
    let mut builder = EntityTypeRegistryBuilder::new(&catalog);
    register_entity_type(&mut builder, Arc::clone(&catalog)).unwrap();
    assert_eq!(
        register_entity_type(&mut builder, Arc::clone(&catalog)),
        Err(EntityError::DuplicateType(DROP_ENTITY_TYPE))
    );
}
