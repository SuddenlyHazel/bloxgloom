use super::*;
use crate::content::{BlockTypeId, OPAQUE};

fn tile() -> LodTile {
    LodTile {
        key: TileKey {
            level: 1,
            x: -2,
            z: 3,
        },
        revision: u64::MAX - 1,
        geometric_error: u32::MAX,
        columns: vec![
            Column {
                coverage: vec![Interval {
                    bottom: -123_456,
                    top: 234_567
                }],
                spans: vec![]
            };
            1024
        ],
        trees: vec![],
    }
}

fn decode(bytes: &[u8], catalog: &Catalog) -> io::Result<LodTile> {
    let mut cursor = Cursor { bytes, offset: 0 };
    let result = read_tile(&mut cursor, catalog)?;
    cursor.done()?;
    Ok(result)
}

fn roundtrip(tile: &LodTile, catalog: &Catalog) -> Vec<u8> {
    let mut bytes = vec![];
    write_tile(&mut bytes, tile, catalog).unwrap();
    assert_eq!(bytes.len(), tile_len(tile));
    assert_eq!(bytes.len(), tile.encoded_bytes());
    assert_eq!(decode(&bytes, catalog).unwrap(), *tile);
    bytes
}

#[test]
fn compact_lod_roundtrips_all_light_nibbles_exact_bounds_and_empty_palette() {
    let catalog = crate::content::catalog();
    let mut tile = tile();
    let empty = roundtrip(&tile, catalog);
    assert_eq!(&empty[23..26], &[0, 0, 1]);
    for sky in 0..16 {
        for glow in 0..16 {
            tile.columns[sky * 16 + glow].spans.push(Span {
                bottom: -123_456,
                top: 234_567,
                state: crate::world::STONE,
                sky: sky as u8,
                glow: glow as u8,
            });
        }
    }
    let bytes = roundtrip(&tile, catalog);
    assert_eq!(&bytes[23..26], &[1, 0, 1]);
    for field in [true, false] {
        let mut invalid = tile.clone();
        if field {
            invalid.columns[0].spans[0].sky = 16;
        } else {
            invalid.columns[0].spans[0].glow = 16;
        }
        assert!(write_tile(&mut vec![], &invalid, catalog).is_err());
    }
}

fn high_catalog(count: usize) -> (Catalog, Vec<BlockStateId>) {
    let mut catalog = Catalog::builtins();
    let template = catalog
        .block_type(BlockTypeId(crate::world::STONE.0))
        .unwrap()
        .clone();
    let mut states = vec![];
    for i in 0..count {
        let mut block = template.clone();
        block.id = BlockTypeId(70_000 + i as u32);
        block.key = format!("test:compact_{i}").into();
        let state = BlockStateId(100_000 + i as u32);
        catalog.register_block(block.clone()).unwrap();
        catalog
            .register_state(state, block.id, vec![], None)
            .unwrap();
        assert_ne!(catalog.block_flags(state) & OPAQUE, 0);
        states.push(state);
    }
    (catalog, states)
}

#[test]
fn compact_lod_retains_high_state_ids_and_switches_width_above_256() {
    let (catalog, states) = high_catalog(257);
    for count in [1, 256, 257] {
        let mut tile = tile();
        // Reverse first occurrence: palette must still be sorted by stable ID.
        for (column, state) in tile.columns.iter_mut().zip(states[..count].iter().rev()) {
            column.spans.push(Span {
                bottom: -100_000,
                top: 200_000,
                state: *state,
                sky: 15,
                glow: 9,
            });
        }
        let bytes = roundtrip(&tile, &catalog);
        assert_eq!(
            u16::from_le_bytes(bytes[23..25].try_into().unwrap()) as usize,
            count
        );
        assert_eq!(bytes[25], if count <= 256 { 1 } else { 2 });
        let decoded_palette = bytes[26..26 + count * 4]
            .chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            decoded_palette,
            states[..count]
                .iter()
                .map(|state| state.0)
                .collect::<Vec<_>>()
        );
        let mut invalid_index = bytes.clone();
        let index = 26 + count * 4 + 4 + 8 + 8;
        if count <= 256 {
            if count < 256 {
                invalid_index[index] = count as u8;
            } else {
                continue;
            } // Every u8 index is legal at this boundary.
        } else {
            invalid_index[index..index + 2].copy_from_slice(&(count as u16).to_le_bytes());
        }
        assert!(decode(&invalid_index, &catalog).is_err());
    }
}

#[test]
fn compact_lod_rejects_noncanonical_palette_bad_material_counts_and_truncation() {
    let catalog = crate::content::catalog();
    let mut tile = tile();
    for (column, state) in tile
        .columns
        .iter_mut()
        .zip([crate::world::STONE, crate::world::WATER])
    {
        column.spans.push(Span {
            bottom: -20,
            top: 20,
            state,
            sky: 3,
            glow: 12,
        });
    }
    let bytes = roundtrip(&tile, catalog);
    let mut corruptions = vec![];
    for width in [0, 2, 3, 255] {
        let mut bad = bytes.clone();
        bad[25] = width;
        corruptions.push(bad);
    }
    for state in [u32::MAX, crate::world::AIR.0] {
        let mut bad = bytes.clone();
        bad[26..30].copy_from_slice(&state.to_le_bytes());
        corruptions.push(bad);
    }
    let mut duplicate = bytes.clone();
    duplicate.copy_within(26..30, 30);
    corruptions.push(duplicate);
    let mut reversed = bytes.clone();
    let first = reversed[26..30].to_vec();
    reversed.copy_within(30..34, 26);
    reversed[30..34].copy_from_slice(&first);
    corruptions.push(reversed);
    let mut oversized = bytes.clone();
    oversized[23..25].copy_from_slice(&u16::MAX.to_le_bytes());
    corruptions.push(oversized);
    // Both columns now refer to palette0; unused palette1 is not canonical.
    let mut unused = bytes.clone();
    let second_index = 34 + 4 + 8 + 10 + 4 + 8 + 8;
    unused[second_index] = 0;
    corruptions.push(unused);
    for bad in corruptions {
        assert!(decode(&bad, catalog).is_err());
    }
    // Every proper prefix is incomplete, including palette/count/bounds/index/light/tree-count.
    for length in 0..bytes.len() {
        assert!(
            decode(&bytes[..length], catalog).is_err(),
            "accepted prefix {length}"
        );
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode(&trailing, catalog).is_err());
}

#[test]
fn compact_lod_accepts_exact_60_kib_and_rejects_next_span_before_payload_read() {
    let catalog = crate::content::catalog();
    let mut tile = tile();
    for (index, column) in tile.columns.iter_mut().enumerate() {
        column.coverage = vec![Interval { bottom: 0, top: 64 }];
        for span in 0..if index < 816 { 5 } else { 4 } {
            column.spans.push(Span {
                bottom: span * 2,
                top: span * 2 + 1,
                state: crate::world::STONE,
                sky: 0,
                glow: 15,
            });
        }
    }
    assert_eq!(tile.encoded_bytes(), MAX_TILE_BYTES);
    let bytes = roundtrip(&tile, catalog);
    let last_count = bytes.len() - 2 - (4 + 8 + 4 * 10) + 2;
    let mut truncated = bytes[..last_count + 2].to_vec();
    truncated[last_count..last_count + 2].copy_from_slice(&5u16.to_le_bytes());
    let error = decode(&truncated, catalog).unwrap_err();
    assert!(error.to_string().contains("allocation cap"), "{error}");
    tile.columns[1023].spans.push(Span {
        bottom: 10,
        top: 11,
        state: crate::world::STONE,
        sky: 0,
        glow: 0,
    });
    assert!(write_tile(&mut vec![], &tile, catalog).is_err());
}

#[test]
fn compact_lod_wire_version_rejects_previous_codec_before_decoding() {
    let catalog = crate::content::catalog();
    let mut bytes = vec![];
    crate::protocol::write_client_with_catalog(
        &mut bytes,
        &ClientMessage::LodRequest {
            request: 8,
            key: tile().key,
        },
        catalog,
    )
    .unwrap();
    assert_eq!(bytes[4], 35);
    bytes[4] = 34;
    let error = crate::protocol::read_client_with_catalog(bytes.as_slice(), catalog).unwrap_err();
    assert!(error.to_string().contains("version"), "{error}");
}
