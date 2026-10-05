use super::*;

#[test]
fn storage_automation_faces_change_entity_save_and_handshake_identity() {
    let catalog = crate::server::catalog_with_extension(
        Catalog::builtins(),
        &bloxgloom_lifecycle_fixture::TallStore,
    )
    .unwrap();
    let mut all_faces = catalog.clone();
    all_faces.storage_lifecycles[0].automation_faces =
        Some(bloxgloom_host_api::machine::FACES.into());
    let mut restricted = catalog.clone();
    restricted.storage_lifecycles[0].automation_faces = Some(vec![[0, 1, 0]]);
    let key = bloxgloom_lifecycle_fixture::KEY;
    let identity = |catalog: &Catalog| {
        catalog
            .identities()
            .into_iter()
            .find(|(kind, _, entity, _)| *kind == b'E' && *entity == key)
            .unwrap()
            .3
    };
    assert_eq!(identity(&catalog), identity(&all_faces));
    assert_ne!(identity(&catalog), identity(&restricted));
    assert_eq!(catalog.fingerprint(), all_faces.fingerprint());
    assert_ne!(catalog.fingerprint(), restricted.fingerprint());
    let manifest = ContentManifest::from_catalog(&catalog);
    assert!(manifest.resolve_catalog(&restricted).is_err());
}

#[test]
fn wood_axis_compiles_six_face_textures_once() {
    let catalog = Catalog::builtins();
    for (state, cap_axis) in [
        (world::WOOD_X, 0usize),
        (world::WOOD, 1usize),
        (world::WOOD_Z, 2usize),
    ] {
        let state = catalog.state(state).unwrap();
        for axis in 0..3 {
            for side in [-1, 1] {
                let expected = if axis == cap_axis {
                    if side > 0 {
                        state.textures.top
                    } else {
                        state.textures.bottom
                    }
                } else {
                    state.textures.side
                };
                assert_eq!(state.face_texture(axis, side), Some(expected));
            }
        }
        assert_eq!(state.face_texture(3, 1), None);
    }
}

#[test]
fn nonmonotonic_registration_never_truncates_prior_definitions() {
    let mut catalog = Catalog::builtins();
    let mut high = catalog.block_type(BlockTypeId(3)).unwrap().clone();
    high.id = BlockTypeId(70_000);
    high.key = "test:high".into();
    catalog.register_block(high).unwrap();
    let mut low = catalog.block_type(BlockTypeId(3)).unwrap().clone();
    low.id = BlockTypeId(17);
    low.key = "test:low".into();
    catalog.register_block(low).unwrap();
    assert!(catalog.block_type(BlockTypeId(70_000)).is_some());
    catalog
        .register_state(BlockStateId(70_001), BlockTypeId(70_000), vec![], None)
        .unwrap();
    catalog
        .register_state(BlockStateId(258), BlockTypeId(17), vec![], None)
        .unwrap();
    assert!(catalog.state(BlockStateId(70_001)).is_some());
    let texture = catalog.block_type(BlockTypeId(3)).unwrap().textures.top;
    catalog
        .register_item(ItemDef {
            id: ItemId(70_002),
            key: "test:high".into(),
            name: "HIGH".into(),
            swatch: [1.0; 4],
            texture,
            placeable: Some(BlockStateId(70_001)),
            sprite: false,
        })
        .unwrap();
    catalog
        .register_item(ItemDef {
            id: ItemId(17),
            key: "test:low".into(),
            name: "LOW".into(),
            swatch: [1.0; 4],
            texture,
            placeable: Some(BlockStateId(258)),
            sprite: false,
        })
        .unwrap();
    assert!(catalog.item(ItemId(70_002)).is_some());
    assert_eq!(
        catalog.primary_block_item(BlockStateId(70_001)),
        Some(ItemId(70_002))
    );
    catalog
        .register_entity_type(EntityTypeDef {
            id: EntityTypeId(70_003),
            key: "test:high".into(),
            schema_version: 1,
            schema_fingerprint: 12,
        })
        .unwrap();
    catalog
        .register_entity_type(EntityTypeDef {
            id: EntityTypeId(7),
            key: "test:low".into(),
            schema_version: 1,
            schema_fingerprint: 13,
        })
        .unwrap();
    assert!(catalog.entity_type(EntityTypeId(70_003)).is_some());
    assert!(catalog.validate().is_ok());
    let manifest = ContentManifest::from_catalog(&catalog);
    assert!(manifest.entries.iter().any(|entry| entry.id == 70_003));
}

#[test]
fn builtin_catalog_preserves_default_state_and_item_ids() {
    let catalog = Catalog::builtins();
    assert_eq!(catalog.textures().len(), 52);
    for id in 0..=world::MAX_BUILTIN_BLOCK.0 {
        let state = BlockStateId(id);
        assert!(catalog.block(state).is_some());
        assert_eq!(catalog.block_flags(state), BUILTIN_FLAGS[id as usize]);
        assert_eq!(catalog.state(state).unwrap().block_type, BlockTypeId(id));
        if state != world::AIR {
            assert_eq!(catalog.item(ItemId(id)).unwrap().placeable, Some(state));
        }
    }
    for state in [world::WOOD, world::WOOD_X, world::WOOD_Z, world::LEAVES] {
        assert_ne!(catalog.block_flags(state) & FLAMMABLE, 0);
    }
    for state in 0..=world::MAX_BUILTIN_BLOCK.0 {
        let state = BlockStateId(state);
        if ![world::WOOD, world::LEAVES].contains(&state) {
            assert_eq!(catalog.block_flags(state) & FLAMMABLE, 0);
        }
    }
    assert!(catalog.block(BlockStateId(17)).is_none());
    for (state, axis) in [
        (world::WOOD_X, "x"),
        (world::WOOD, "y"),
        (world::WOOD_Z, "z"),
    ] {
        let definition = catalog.state(state).unwrap();
        assert_eq!(definition.block_type, BlockTypeId(world::WOOD.0));
        assert_eq!(definition.key, format!("bloxgloom:wood[axis={axis}]"));
        assert_eq!(definition.flags, catalog.state(world::WOOD).unwrap().flags);
    }
    assert!(catalog.entity_type(EntityTypeId(1)).is_some());
    assert!(catalog.entity_type(EntityTypeId(2)).is_some());
    assert!(catalog.entity_type(KILN_ENTITY_TYPE).is_some());
    assert_eq!(
        catalog.item(KILN_ITEM).unwrap().placeable,
        Some(KILN_DEFAULT_STATE)
    );
    let lit_default = catalog
        .state_with_property(KILN_DEFAULT_STATE, "lit", "true")
        .unwrap();
    assert_eq!(catalog.emission(lit_default), 12);
}

#[test]
fn registration_rejects_collisions_and_invalid_state_schema() {
    let mut catalog = Catalog::builtins();
    let builtin_fingerprint = catalog.fingerprint();
    let layer = catalog
        .register_texture(TextureDef {
            key: "example:marble".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })
        .unwrap();
    assert_eq!(layer, TextureId(52));
    let marble = BlockDef {
        id: BlockTypeId(17),
        key: "example:marble".into(),
        name: "MARBLE".into(),
        swatch: [0.9, 0.85, 0.8, 1.0],
        textures: BlockTextures {
            top: layer,
            side: layer,
            bottom: layer,
        },
        solid: true,
        opaque: true,
        fluid: false,
        cutout: false,
        plant: false,
        replaceable: false,
        supports_plant: false,
        flammable: false,
        emission: 0,
        sky_attenuation: 0,
        reflectance: [180, 180, 180],
        properties: Vec::new(),
    };
    catalog.register_block(marble.clone()).unwrap();
    assert_eq!(
        catalog.register_block(marble),
        Err(RegistrationError::DuplicateId)
    );
    let state = BlockStateId(258);
    catalog
        .register_state(state, BlockTypeId(17), Vec::new(), None)
        .unwrap();
    assert_eq!(
        catalog.register_state(state, BlockTypeId(17), Vec::new(), None),
        Err(RegistrationError::DuplicateId)
    );
    assert_eq!(
        catalog.register_state(
            BlockStateId(259),
            BlockTypeId(17),
            vec![("axis".into(), "x".into())],
            None
        ),
        Err(RegistrationError::InvalidState)
    );
    catalog
        .register_item(ItemDef {
            id: ItemId(131),
            key: "example:marble".into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.85, 0.8, 1.0],
            texture: layer,
            placeable: Some(state),
            sprite: false,
        })
        .unwrap();
    assert_eq!(catalog.primary_block_item(state), Some(ItemId(131)));
    assert_ne!(catalog.fingerprint(), builtin_fingerprint);
    assert_eq!(
        catalog.register_texture(TextureDef {
            key: "example:marble".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        }),
        Err(RegistrationError::DuplicateKey)
    );
}

#[test]
fn sky_attenuation_is_validated_and_part_of_catalog_identity() {
    let mut baseline = Catalog::builtins();
    let mut leaves = baseline.block(world::LEAVES).unwrap().clone();
    leaves.id = BlockTypeId(65_530);
    leaves.key = "fixture:absorbing_canopy".into();
    leaves.sky_attenuation = 16;
    assert_eq!(
        baseline.register_block(leaves.clone()),
        Err(RegistrationError::InvalidDefinition)
    );
    leaves.sky_attenuation = 4;
    baseline.register_block(leaves.clone()).unwrap();
    let fingerprint = baseline.definition_fingerprint(b'B', leaves.id.0);
    let mut other = Catalog::builtins();
    leaves.sky_attenuation = 3;
    other.register_block(leaves.clone()).unwrap();
    assert_ne!(fingerprint, other.definition_fingerprint(b'B', leaves.id.0));
    assert_eq!(other.sky_attenuation(world::STONE), 15);
    assert_eq!(other.sky_attenuation(world::AIR), 0);
    assert_eq!(other.sky_attenuation(world::LEAVES), 2);
}
