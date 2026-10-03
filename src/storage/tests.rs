use super::*;
use crate::content::{
    BlockDef, BlockStateId, BlockTextures, BlockTypeId, ItemDef, ItemId, TextureDef,
};
use std::borrow::Cow;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn temporary_root(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("bloxgloom-{label}-{}-{stamp}", std::process::id()));
    fs::create_dir(&root).unwrap();
    root
}

fn with_extra_block(key: &str) -> Catalog {
    with_extra_block_ids(key, 65_536, 65_537)
}

fn with_extra_block_ids(key: &str, state_id: u32, item_id: u32) -> Catalog {
    let mut catalog = Catalog::builtins();
    let texture = catalog
        .register_texture(TextureDef {
            key: "example:marble_tile".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
            emission_strength: 0.0,
        })
        .unwrap();
    catalog
        .register_block(BlockDef {
            id: BlockTypeId(16),
            key: key.to_owned().into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.9, 0.9, 1.0],
            textures: BlockTextures {
                top: texture,
                side: texture,
                bottom: texture,
            },
            solid: true,
            opaque: true,
            cutout: false,
            plant: false,
            replaceable: false,
            supports_plant: false,
            flammable: false,
            emission: 0,
            reflectance: [180, 180, 180],
            properties: Vec::new(),
        })
        .unwrap();
    catalog
        .register_state(BlockStateId(state_id), BlockTypeId(16), Vec::new(), None)
        .unwrap();
    catalog
        .register_item(ItemDef {
            id: ItemId(item_id),
            key: key.to_owned().into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.9, 0.9, 1.0],
            texture,
            placeable: Some(BlockStateId(state_id)),
            sprite: false,
        })
        .unwrap();
    catalog
}

#[test]
fn content_map_preserves_wide_assignments_and_rejects_reassignment() {
    let root = temporary_root("content-map");
    let base = Catalog::builtins();
    let base_manifest = ContentManifest::from_catalog(&base);
    let next_id = |kind| {
        base_manifest
            .entries
            .iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.id)
            .max()
            .unwrap()
            + 1
    };
    let first_block_id = next_id(b'B');
    let first_state_id = next_id(b'S');
    let first_item_id = next_id(b'I');
    verify_content_map_with(&root, true, &base).unwrap();
    let extended = with_extra_block("example:marble");
    verify_content_map_with(&root, false, &extended).unwrap();
    let saved = ContentManifest::decode(&fs::read(root.join(CONTENT_MAP)).unwrap()).unwrap();
    assert!(saved.entries.iter().any(|entry| entry.kind == b'S'
        && entry.id == first_state_id
        && entry.key == "example:marble"));
    assert!(saved.entries.iter().any(|entry| entry.kind == b'I'
        && entry.id == first_item_id
        && entry.key == "example:marble"));
    verify_content_map_with(&root, false, &base).unwrap(); // Removed IDs remain reserved.
    let replacement =
        resolve_content_map_with(&root, false, &with_extra_block("other:marble")).unwrap();
    assert!(
        replacement
            .block_type(BlockTypeId(first_block_id))
            .is_none()
    );
    assert_eq!(
        replacement
            .block_type(BlockTypeId(first_block_id + 1))
            .unwrap()
            .key,
        "other:marble"
    );
    let saved = ContentManifest::decode(&fs::read(root.join(CONTENT_MAP)).unwrap()).unwrap();
    assert!(saved.entries.iter().any(|entry| entry.kind == b'B'
        && entry.id == first_block_id
        && entry.key == "example:marble"));
    assert!(saved.entries.iter().any(|entry| entry.kind == b'B'
        && entry.id == first_block_id + 1
        && entry.key == "other:marble"));
    let mut corrupt = fs::read(root.join(CONTENT_MAP)).unwrap();
    corrupt[10] ^= 1;
    fs::write(root.join(CONTENT_MAP), corrupt).unwrap();
    assert!(verify_content_map_with(&root, false, &extended).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incompatible_entity_contract_reports_saved_evidence_before_any_file_mutation() {
    use crate::content::{EntityTypeDef, EntityTypeId};
    let root = temporary_root("content-contract-reject");
    let catalog_with_schema = |revision| {
        let mut catalog = Catalog::builtins();
        catalog
            .register_entity_type(EntityTypeDef {
                id: EntityTypeId(65_536),
                key: "example:creature".into(),
                schema_version: revision,
                schema_fingerprint: u64::from(revision),
            })
            .unwrap();
        Arc::new(catalog)
    };
    drop(Storage::with_catalog(&root, 42, catalog_with_schema(1)).unwrap());
    fs::remove_file(root.join(WORLD_LOCK)).unwrap();
    // Preserve representative save files, including files the content loader
    // never interprets. Rejection must not create a lock or temporary map.
    fs::write(root.join("drops.bin"), b"saved drops").unwrap();
    fs::create_dir(root.join("inventories")).unwrap();
    fs::write(root.join("inventories/profile.bin"), b"saved inventory").unwrap();
    let metadata = fs::read(root.join(WORLD_META)).unwrap();
    let content = fs::read(root.join(CONTENT_MAP)).unwrap();
    let error = Storage::with_catalog(&root, 42, catalog_with_schema(2)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    let text = error.to_string();
    assert!(text.contains("entity 'example:creature'"), "{text}");
    assert!(
        text.contains("namespace 'example'") && text.contains("saved ID 65536"),
        "{text}"
    );
    let saved = ContentManifest::decode(&content)
        .unwrap()
        .entries
        .into_iter()
        .find(|entry| entry.key == "example:creature")
        .unwrap();
    assert!(
        text.contains(&format!("saved {:016x}", saved.schema_fingerprint)),
        "{text}"
    );
    assert!(
        text.contains("new world directory") && text.contains("existing save left unchanged"),
        "{text}"
    );
    assert_eq!(fs::read(root.join(WORLD_META)).unwrap(), metadata);
    assert_eq!(fs::read(root.join(CONTENT_MAP)).unwrap(), content);
    assert_eq!(fs::read(root.join("drops.bin")).unwrap(), b"saved drops");
    assert_eq!(
        fs::read(root.join("inventories/profile.bin")).unwrap(),
        b"saved inventory"
    );
    assert!(!root.join(WORLD_LOCK).exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn wide_sparse_edits_round_trip_and_reject_reordering() {
    let root = temporary_root("wide-edits");
    let catalog = Arc::new(with_extra_block("example:marble"));
    let storage = Storage::with_catalog(&root, 42, catalog).unwrap();
    let edits = SavedEdits {
        version: 7,
        blocks: BTreeMap::from([(1, BlockStateId(65_536)), (4_095, BlockStateId(257))]),
    };
    let bytes = storage.encode_snapshot(&edits).unwrap().unwrap();
    assert_eq!(&bytes[..6], b"BGED\x03\x00");
    assert_eq!(storage.decode_snapshot(Some(&bytes)).unwrap(), edits);
    let mut reversed = bytes.clone();
    reversed[HEADER_LEN..HEADER_LEN + 6].copy_from_slice(&bytes[HEADER_LEN + 6..HEADER_LEN + 12]);
    reversed[HEADER_LEN + 6..HEADER_LEN + 12].copy_from_slice(&bytes[HEADER_LEN..HEADER_LEN + 6]);
    let checksum_at = reversed.len() - 4;
    let sum = checksum(&reversed[..checksum_at]);
    reversed[checksum_at..].copy_from_slice(&sum.to_le_bytes());
    assert!(storage.decode_snapshot(Some(&reversed)).is_err());
    assert!(
        storage
            .decode_snapshot(Some(&bytes[..bytes.len() - 1]))
            .is_err()
    );
    let key = ChunkKey { x: -1, y: 2, z: 0 };
    storage.save(key, &edits).unwrap();
    drop(storage);
    let reopened = Storage::with_catalog(
        &root,
        42,
        Arc::new(with_extra_block_ids("example:marble", 258, 131)),
    )
    .unwrap();
    assert!(reopened.catalog_arc().state(BlockStateId(65_536)).is_some());
    assert_eq!(reopened.load(key).unwrap(), edits);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn active_world_lock_excludes_a_second_writer_and_releases_on_drop() {
    let root = temporary_root("world-lock");
    let first = Storage::new(&root, 7).unwrap();
    let error = Storage::new(&root, 7).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    let clone = first.clone();
    drop(first);
    assert_eq!(
        Storage::new(&root, 7).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(clone);
    Storage::new(&root, 7).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn old_world_is_rejected_without_creating_a_lock_or_rewriting_data() {
    let root = temporary_root("old-world-reject");
    let mut old_meta = b"BGWD".to_vec();
    old_meta.extend(TERRAIN_GENERATOR_VERSION.to_le_bytes());
    old_meta.extend(7u64.to_le_bytes());
    fs::write(root.join(WORLD_META), &old_meta).unwrap();
    let error = Storage::new(&root, 7).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("no automatic upgrade"));
    assert_eq!(fs::read(root.join(WORLD_META)).unwrap(), old_meta);
    assert!(!root.join(WORLD_LOCK).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn incomplete_conversion_cannot_be_opened_as_a_world() {
    let root = temporary_root("incomplete-conversion");
    fs::write(root.join(CONVERSION_INCOMPLETE), b"conversion in progress").unwrap();
    assert_eq!(
        Storage::new(&root, 7).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(!root.join(WORLD_META).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hidden_partial_conversion_stage_cannot_be_opened_even_after_marker_removal() {
    let root = temporary_root("partial-stage").join(".world.migration-v4.1.partial");
    fs::create_dir(&root).unwrap();
    assert_eq!(
        Storage::new(&root, 7).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(!root.join(WORLD_META).exists());
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
