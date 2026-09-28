use super::*;

#[test]
fn builtin_player_rules_survive_save_and_connection_reconstruction() {
    let local = Catalog::builtins();
    let manifest = ContentManifest::from_catalog(&local);
    let mut saved = ContentManifest::decode(&manifest.encode().unwrap()).unwrap();
    let (world, changed) = saved.resolve_world_catalog(&local).unwrap();
    assert!(!changed);
    assert_eq!(
        world.player_rules(),
        bloxgloom_host_api::player::BUILTIN_RULES
    );
    let connected = ContentManifest::from_catalog(&world)
        .resolve_catalog(&local)
        .unwrap();
    assert_eq!(connected.player_rules(), world.player_rules());
    assert_eq!(connected.fingerprint(), world.fingerprint());
    assert_eq!(ContentManifest::from_catalog(&connected), manifest);
}

#[test]
fn unnegotiated_player_override_is_rejected_not_reconstructed_as_builtin() {
    use bloxgloom_host_api::player::{BUILTIN_RULES, PlayerRules};
    let mut local = Catalog::builtins();
    let mut saved = ContentManifest::from_catalog(&local);
    local.player_rules = PlayerRules::new(
        BUILTIN_RULES.body(),
        BUILTIN_RULES.motion(),
        BUILTIN_RULES.spawn(),
        1.5,
    )
    .unwrap();
    assert_eq!(
        local.validate(),
        Err(super::super::RegistrationError::InvalidDefinition)
    );
    assert!(saved.resolve_world_catalog(&local).is_err());
    assert!(saved.resolve_catalog(&local).is_err());
}

#[test]
fn selected_player_identity_commits_key_revision_and_every_compiled_field() {
    use crate::content::player::Selection;
    use bloxgloom_host_api::player::{BUILTIN_RULES, PlayerRules};
    let selection = Selection {
        key: "demo:player".into(),
        revision: 1,
        rules: BUILTIN_RULES,
    };
    let mut local = Catalog::builtins();
    let builtin_identity = local.fingerprint();
    local.select_player_rules(selection.clone()).unwrap();
    assert_ne!(
        local.fingerprint(),
        builtin_identity,
        "explicit selections are identities even with builtin values"
    );
    let manifest = ContentManifest::from_catalog(&local);
    let bytes = manifest.encode().unwrap();
    let mut saved = ContentManifest::decode(&bytes).unwrap();
    let (resolved, changed) = saved.resolve_world_catalog(&local).unwrap();
    assert!(!changed);
    assert_eq!(resolved.fingerprint(), local.fingerprint());
    assert_eq!(resolved.player_rules(), local.player_rules());
    assert!(local.select_player_rules(selection.clone()).is_err());
    let mut alternatives = vec![Catalog::builtins()];
    for (offset, value) in [
        (0, 0.4f32),
        (4, 0.1),
        (8, 1.0),
        (12, 1.8),
        (16, 7.0),
        (20, 1.5),
    ] {
        let mut bytes = BUILTIN_RULES.canonical_bytes();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        let mut candidate = Catalog::builtins();
        candidate
            .select_player_rules(Selection {
                rules: PlayerRules::from_canonical_bytes(bytes).unwrap(),
                ..selection.clone()
            })
            .unwrap();
        alternatives.push(candidate);
    }
    for (offset, bytes) in [
        (24, 11f64.to_le_bytes().to_vec()),
        (32, 33i32.to_le_bytes().to_vec()),
        (36, 129i32.to_le_bytes().to_vec()),
    ] {
        let mut encoded = BUILTIN_RULES.canonical_bytes();
        encoded[offset..offset + bytes.len()].copy_from_slice(&bytes);
        let mut candidate = Catalog::builtins();
        candidate
            .select_player_rules(Selection {
                rules: PlayerRules::from_canonical_bytes(encoded).unwrap(),
                ..selection.clone()
            })
            .unwrap();
        alternatives.push(candidate);
    }
    for changed in [
        Selection {
            key: "demo:other".into(),
            ..selection.clone()
        },
        Selection {
            revision: 2,
            ..selection
        },
    ] {
        let mut candidate = Catalog::builtins();
        candidate.select_player_rules(changed).unwrap();
        alternatives.push(candidate);
    }
    for candidate in alternatives {
        assert_ne!(candidate.fingerprint(), local.fingerprint());
        assert!(manifest.resolve_catalog(&candidate).is_err());
        assert!(saved.clone().resolve_world_catalog(&candidate).is_err());
        assert_eq!(saved.encode().unwrap(), bytes);
    }
}

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
