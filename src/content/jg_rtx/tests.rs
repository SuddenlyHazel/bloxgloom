use super::*;

#[test]
fn import_preserves_original_assignments_and_compiles_every_block_item() {
    let catalog = Catalog::builtins();
    catalog.validate().unwrap();
    assert_eq!(
        catalog.texture(TextureId(0)).unwrap().key,
        "bloxgloom:grass_top"
    );
    assert_eq!(
        catalog.texture(TextureId(27)).unwrap().key,
        "bloxgloom:grass_top_n"
    );
    assert_eq!(
        catalog.texture(TextureId(51)).unwrap().key,
        "bloxgloom:water"
    );
    assert_eq!(
        catalog.state(world::WOOD_X).unwrap().key,
        "bloxgloom:wood[axis=x]"
    );
    let mut ids = HashSet::new();
    for b in &import().blocks {
        assert!(ids.insert(b.id), "duplicate persistent ID for {}", b.key);
        let block = catalog
            .block_by_key(&format!("bloxgloom:{}", b.key))
            .unwrap();
        assert_eq!(block, BlockTypeId(BLOCK_BASE + b.id));
        let item_id = catalog
            .item_by_key(&format!("bloxgloom:{}", b.key))
            .unwrap();
        let item = catalog.item(item_id).unwrap();
        assert_eq!(item.id, ItemId(BLOCK_BASE + b.id));
        let placement = item.placeable.unwrap();
        assert_eq!(placement, BlockStateId(STATE_BASE + STATE_STRIDE * b.id));
        assert_eq!(catalog.state(placement).unwrap().block_type, block);
        assert_eq!(catalog.primary_block_item(placement), Some(item.id));
        // Azalea shrubs use solid cutout foliage geometry but must not decay
        // as disconnected tree canopy.
        assert_eq!(
            is_leaf(&catalog, placement),
            b.kind == Kind::Leaves && !matches!(b.key.as_str(), "azalea" | "flowering_azalea")
        );
        if b.kind == Kind::Log {
            assert_eq!(is_log(&catalog, placement), b.flammable);
            for (value, axis) in [("x", 0), ("y", 1), ("z", 2)] {
                let id = catalog
                    .state_with_property(placement, "axis", value)
                    .unwrap();
                let s = catalog.state(id).unwrap();
                assert_eq!(s.face_texture(axis, 1), Some(s.textures.top));
                assert_eq!(s.face_texture(axis, -1), Some(s.textures.bottom));
            }
        }
        if b.kind == Kind::TallPlant {
            let (lower, upper) = tall_pair(&catalog, placement).unwrap();
            assert_eq!(lower, placement);
            assert_eq!(catalog.primary_block_item(upper), Some(item.id));
            assert_ne!(
                catalog.state(lower).unwrap().textures.side,
                catalog.state(upper).unwrap().textures.side
            );
        }
    }
}

#[test]
fn imported_catalog_round_trips_the_world_manifest_and_handshake() {
    let catalog = Catalog::builtins();
    let manifest = ContentManifest::from_catalog(&catalog);
    let mut decoded = ContentManifest::decode(&manifest.encode().unwrap()).unwrap();
    assert_eq!(manifest, decoded);
    let (restored, changed) = decoded.resolve_world_catalog(&catalog).unwrap();
    assert!(!changed);
    assert_eq!(catalog.fingerprint(), restored.fingerprint());
    assert_eq!(ContentManifest::from_catalog(&restored), manifest);
    let dirt = catalog.texture_key("bloxgloom:dirt").unwrap();
    let stick = catalog.texture_key("bloxgloom:stick").unwrap();
    assert!(catalog.is_lab_pbr_texture(dirt));
    assert!(restored.is_lab_pbr_texture(dirt));
    assert!(!catalog.is_lab_pbr_texture(stick));
}

#[test]
fn source_art_thumbnails_cover_new_blocks_plants_and_replaced_legacy_items() {
    let catalog = Catalog::builtins();
    for item in catalog.items() {
        assert!(
            catalog.item_icon(item.id).is_some(),
            "missing palette icon for {}",
            item.key
        );
    }
    for (key, name) in [
        ("oak_planks", "OAK PLANKS"),
        ("poppy", "POPPY"),
        ("red_flower", "RED FLOWER"),
        ("wood", "WOOD"),
    ] {
        let item = catalog.item_by_key(&format!("bloxgloom:{key}")).unwrap();
        assert_eq!(catalog.item(item).unwrap().name, name);
        let icon = catalog.item_icon(item).expect("imported texture thumbnail");
        icon.validate().unwrap();
        assert!(icon.rows.iter().any(|row| row.bytes().any(|c| c != b'.')));
        assert!(icon.palette.len() > 1, "preserve material color detail");
    }
    let flower = catalog
        .item_icon(catalog.item_by_key("bloxgloom:poppy").unwrap())
        .unwrap();
    assert!(
        flower.rows.iter().any(|row| row.contains('.')),
        "plant silhouette preserves transparency"
    );
    assert!(catalog.item_icon(crate::items::SEEDS).is_some());
    assert!(catalog.item_icon(crate::items::STICK).is_some());
}
