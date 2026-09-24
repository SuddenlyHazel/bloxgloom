use super::*;
use crate::world::{STONE, WOOD};

fn content_map_v1() -> Vec<u8> {
    let mut bytes = b"BGCM".to_vec();
    bytes.extend(1u16.to_le_bytes());
    let mut entries = Catalog::builtins()
        .identities()
        .into_iter()
        .filter(|(kind, _, _, _)| matches!(kind, b'B' | b'I'))
        .map(|(kind, id, key, _)| (kind, id as u8, key.to_owned()))
        .collect::<Vec<_>>();
    entries.sort_by_key(|(kind, id, _)| (*kind, *id));
    bytes.extend((entries.len() as u16).to_le_bytes());
    for (kind, id, key) in entries {
        bytes.extend([kind, id, key.len() as u8]);
        bytes.extend(key.as_bytes());
    }
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

fn bged_v2(seed: u64, records: &[(u16, u8)]) -> Vec<u8> {
    let mut bytes = b"BGED".to_vec();
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(TERRAIN_GENERATOR_VERSION.to_le_bytes());
    bytes.extend(seed.to_le_bytes());
    bytes.extend(12u64.to_le_bytes());
    bytes.extend((records.len() as u16).to_le_bytes());
    for &(index, block) in records {
        bytes.extend(index.to_le_bytes());
        bytes.push(block);
    }
    bytes.extend(checksum(&bytes).to_le_bytes());
    bytes
}

#[test]
fn old_metadata_and_map_are_read_only_and_strict() {
    let seed = 0x0123_4567_89ab_cdefu64;
    let mut meta = b"BGWD".to_vec();
    meta.extend(TERRAIN_GENERATOR_VERSION.to_le_bytes());
    meta.extend(seed.to_le_bytes());
    assert_eq!(decode_world_meta_v4(&meta).unwrap(), seed);
    meta[5] ^= 1;
    assert!(decode_world_meta_v4(&meta).is_err());

    let source = content_map_v1();
    let ids = LegacyIdMap::from_content_map_v1(Some(&source), &Catalog::builtins()).unwrap();
    assert_eq!(ids.state(STONE.0 as u8), Some(STONE));
    assert_eq!(ids.state(WOOD.0 as u8), Some(WOOD));
    assert!(ids.item(0).is_none());
    assert!(LegacyIdMap::from_content_map_v1(None, &Catalog::builtins()).is_ok());
    let mut corrupted = source.clone();
    corrupted[12] ^= 1;
    assert!(LegacyIdMap::from_content_map_v1(Some(&corrupted), &Catalog::builtins()).is_err());
    assert!(
        LegacyIdMap::from_content_map_v1(Some(&source[..source.len() - 1]), &Catalog::builtins())
            .is_err()
    );
}

#[test]
fn legacy_ids_resolve_to_remapped_destination() {
    let local = Catalog::builtins();
    let mut manifest = ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        if (entry.kind, entry.key.as_str()) == (b'S', "bloxgloom:stone") {
            entry.id = 65_537;
        }
        if (entry.kind, entry.key.as_str()) == (b'I', "bloxgloom:stone") {
            entry.id = 65_538;
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let destination = manifest.resolve_catalog(&local).unwrap();
    let ids = LegacyIdMap::from_content_map_v1(Some(&content_map_v1()), &destination).unwrap();
    assert_eq!(ids.state(STONE.0 as u8), Some(BlockStateId(65_537)));
    assert_eq!(ids.item(STONE.0 as u8), Some(ItemId(65_538)));
    assert_eq!(ids.state(WOOD.0 as u8), Some(WOOD));
}

#[test]
fn legacy_sparse_edits_map_and_reject_bad_records() {
    let ids = LegacyIdMap::from_content_map_v1(None, &Catalog::builtins()).unwrap();
    let seed = 77;
    let bytes = bged_v2(seed, &[(0, STONE.0 as u8), (4_095, WOOD.0 as u8)]);
    let saved = decode_bged_v2(&bytes, seed, &ids).unwrap();
    assert_eq!(saved.version, 12);
    assert_eq!(saved.blocks[&0], STONE);
    assert_eq!(saved.blocks[&4_095], WOOD);
    assert!(decode_bged_v2(&bytes, seed + 1, &ids).is_err());
    assert!(decode_bged_v2(&bytes[..bytes.len() - 1], seed, &ids).is_err());
    let mut bad_checksum = bytes.clone();
    bad_checksum[26] ^= 1;
    assert!(decode_bged_v2(&bad_checksum, seed, &ids).is_err());
    assert!(decode_bged_v2(&bged_v2(seed, &[(4, 3), (4, 9)]), seed, &ids).is_err());
    assert!(decode_bged_v2(&bged_v2(seed, &[(5, 3), (4, 9)]), seed, &ids).is_err());
    assert!(decode_bged_v2(&bged_v2(seed, &[(0, 255)]), seed, &ids).is_err());
}
