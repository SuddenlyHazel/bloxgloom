use super::*;

#[test]
fn builtin_catalog_preserves_world_and_item_ids() {
    let catalog = Catalog::builtins();
    assert_eq!(catalog.textures().len(), 20);
    for id in 0..=world::MAX_BUILTIN_BLOCK {
        assert!(catalog.block(id).is_some());
        assert_eq!(catalog.block_flags(id), BUILTIN_FLAGS[id as usize]);
        if id != world::AIR {
            assert_eq!(catalog.item(id).unwrap().placeable, Some(id));
        }
    }
    assert!(catalog.block(16).is_none());
    assert!(
        catalog
            .item(crate::items::SEEDS)
            .unwrap()
            .placeable
            .is_none()
    );
}

#[test]
fn custom_content_registers_before_freeze_and_rejects_collisions() {
    let mut catalog = Catalog::builtins();
    let builtin_fingerprint = catalog.fingerprint();
    let layer = catalog
        .register_texture(TextureDef {
            key: "example:marble".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
        })
        .unwrap();
    assert_eq!(layer, 20);
    let marble = BlockDef {
        id: 16,
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
        cutout: false,
        plant: false,
        replaceable: false,
        supports_plant: false,
        emission: 0,
        reflectance: [180, 180, 180],
    };
    catalog.register_block(marble.clone()).unwrap();
    assert_eq!(
        catalog.register_block(marble),
        Err(RegistrationError::DuplicateId)
    );
    catalog
        .register_item(ItemDef {
            id: 131,
            key: "example:marble".into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.85, 0.8, 1.0],
            texture: layer,
            placeable: Some(16),
            sprite: false,
        })
        .unwrap();
    assert_eq!(catalog.block(16).unwrap().textures.top, 20);
    assert_eq!(catalog.item(131).unwrap().placeable, Some(16));
    assert_eq!(catalog.primary_block_item(16), Some(131));
    catalog
        .register_item(ItemDef {
            id: 20,
            key: "example:alternate_marble".into(),
            name: "ALTERNATE MARBLE".into(),
            swatch: [0.9, 0.85, 0.8, 1.0],
            texture: layer,
            placeable: Some(16),
            sprite: false,
        })
        .unwrap();
    assert_eq!(catalog.primary_block_item(16), Some(20));
    assert_eq!(catalog.textures().len(), 21);
    assert_ne!(catalog.fingerprint(), builtin_fingerprint);
    assert_eq!(
        catalog.register_texture(TextureDef {
            key: "example:marble".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
        }),
        Err(RegistrationError::DuplicateKey)
    );
    assert_eq!(
        catalog.register_texture(TextureDef {
            key: "example:broken".into(),
            png: Cow::Borrowed(b"not a PNG"),
            stitch_edges: false,
            stitch_vertical: false,
            alpha_cutout: false,
        }),
        Err(RegistrationError::InvalidTexture)
    );
}
