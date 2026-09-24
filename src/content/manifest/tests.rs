use super::*;

#[test]
fn manifest_round_trips_wide_ids_and_rejects_corruption() {
    let manifest = ContentManifest {
        entries: vec![
            ContentEntry {
                kind: b'B',
                id: 65_536,
                key: "mod:wide_block".into(),
                schema_fingerprint: 7,
            },
            ContentEntry {
                kind: b'E',
                id: 65_537,
                key: "mod:wide_entity".into(),
                schema_fingerprint: 8,
            },
            ContentEntry {
                kind: b'I',
                id: 65_538,
                key: "mod:wide_item".into(),
                schema_fingerprint: 9,
            },
            ContentEntry {
                kind: b'S',
                id: 65_539,
                key: "mod:wide_block[axis=x]".into(),
                schema_fingerprint: 10,
            },
        ],
    };
    let bytes = manifest.encode().unwrap();
    assert_eq!(&bytes[..6], b"BGCM\x02\x00");
    assert_eq!(ContentManifest::decode(&bytes).unwrap(), manifest);
    assert!(ContentManifest::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut corrupt = bytes.clone();
    corrupt[17] ^= 1;
    assert!(ContentManifest::decode(&corrupt).is_err());
    let mut hostile = bytes;
    hostile[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(ContentManifest::decode(&hostile).is_err());
}

#[test]
fn manifest_rejects_duplicate_reordered_and_sparse_high_water() {
    let entry = ContentEntry {
        kind: b'B',
        id: 256,
        key: "mod:block".into(),
        schema_fingerprint: 1,
    };
    assert!(
        ContentManifest {
            entries: vec![entry.clone(), entry.clone()]
        }
        .encode()
        .is_err()
    );
    assert!(
        ContentManifest {
            entries: vec![
                ContentEntry {
                    id: 257,
                    ..entry.clone()
                },
                entry.clone(),
            ]
        }
        .encode()
        .is_err()
    );
    assert!(
        ContentManifest {
            entries: vec![ContentEntry {
                id: MAX_ASSIGNED_ID,
                ..entry
            },]
        }
        .encode()
        .is_err()
    );
    assert!(
        ContentManifest {
            entries: vec![ContentEntry {
                kind: b'S',
                id: 16,
                key: "mod:block[z=1,a=2]".into(),
                schema_fingerprint: 1,
            }]
        }
        .encode()
        .is_err()
    );
}

#[test]
fn builtin_state_and_entity_identities_are_in_manifest() {
    let catalog = Catalog::builtins();
    let manifest = ContentManifest::from_catalog(&catalog);
    assert!(manifest.entries.iter().any(|entry| entry.kind == b'S'
        && entry.id == 256
        && entry.key == "bloxgloom:wood[axis=x]"));
    assert!(
        manifest
            .entries
            .iter()
            .any(|entry| entry.kind == b'E' && entry.id == 1 && entry.key == "bloxgloom:drop")
    );
    assert_eq!(
        ContentManifest::decode(&manifest.encode().unwrap()).unwrap(),
        manifest
    );
}

#[test]
fn resolving_builtin_catalog_preserves_lit_kiln_compiled_state() {
    let local = Catalog::builtins();
    let manifest = ContentManifest::from_catalog(&local);
    let resolved = manifest.resolve_catalog(&local).unwrap();
    let lit_kiln = BlockStateId(super::super::KILN_DEFAULT_STATE.0 + 1);
    let local_state = local.state(lit_kiln).unwrap();
    let resolved_state = resolved.state(lit_kiln).unwrap();
    assert_eq!(resolved_state.emission, 12);
    assert_eq!(resolved_state.textures.side, local_state.textures.side);
    assert_eq!(resolved_state.face_textures, local_state.face_textures);
    assert_eq!(ContentManifest::from_catalog(&resolved), manifest);
}

#[test]
fn connection_catalog_resolves_server_ids_from_matching_local_keys() {
    let local = Catalog::builtins();
    let mut server = ContentManifest::from_catalog(&local);
    for entry in &mut server.entries {
        if entry.kind == b'B' && entry.key == "bloxgloom:stone" {
            entry.id = 65_536;
        } else if entry.kind == b'S' && entry.key == "bloxgloom:stone" {
            entry.id = 65_537;
        } else if entry.kind == b'I' && entry.key == "bloxgloom:stone" {
            entry.id = 65_538;
        }
    }
    server
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let resolved = server.resolve_catalog(&local).unwrap();
    assert_eq!(
        resolved.block_type(BlockTypeId(65_536)).unwrap().key,
        "bloxgloom:stone"
    );
    assert_eq!(
        resolved.state(BlockStateId(65_537)).unwrap().block_type,
        BlockTypeId(65_536)
    );
    assert_eq!(
        resolved.item(ItemId(65_538)).unwrap().placeable,
        Some(BlockStateId(65_537))
    );
    assert_eq!(ContentManifest::from_catalog(&resolved), server);
    assert!(resolved.state(crate::world::STONE).is_none());
    assert_ne!(resolved.fingerprint(), local.fingerprint());
}
