use super::*;
use crate::protocol::{ClientMessage, ServerMessage};
#[test]
fn bounded_summary_wire_roundtrip_and_allocation_rejection() {
    let catalog = crate::content::catalog();
    let tile = LodTile {
        trees: Vec::new(),
        key: TileKey {
            level: 4,
            x: -1,
            z: 1,
        },
        revision: 19,
        geometric_error: 16,
        columns: vec![
            Column {
                coverage: vec![Interval {
                    bottom: -64,
                    top: 80
                }],
                spans: vec![Span {
                    bottom: -64,
                    top: 20,
                    state: crate::world::STONE,
                    sky: 0,
                    glow: 0
                }]
            };
            1024
        ],
    };
    let message = ServerMessage::LodTile {
        session: 17,
        request: 5,
        tile: tile.clone(),
    };
    let mut bytes = Vec::new();
    crate::protocol::write_server_with_catalog(&mut bytes, &message, catalog).unwrap();
    assert_eq!(bytes.len(), crate::protocol::server_wire_len(&message));
    match crate::protocol::read_server_with_catalog(bytes.as_slice(), catalog).unwrap() {
        ServerMessage::LodTile {
            session: 17,
            request: 5,
            tile: decoded,
        } => assert_eq!(decoded, tile),
        other => panic!("unexpected {other:?}"),
    }
    // First column counts follow frame/version/tag/session/request/tile header.
    let span_count = 4 + 2 + 16 + 9 + 8 + 4 + 2 + 3 + 4 + 2;
    bytes[span_count..span_count + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(crate::protocol::read_server_with_catalog(bytes.as_slice(), catalog).is_err());
    let mut bytes = Vec::new();
    crate::protocol::write_client_with_catalog(
        &mut bytes,
        &ClientMessage::LodRequest {
            request: 5,
            key: tile.key,
        },
        catalog,
    )
    .unwrap();
    assert_eq!(
        crate::protocol::read_client_with_catalog(bytes.as_slice(), catalog).unwrap(),
        ClientMessage::LodRequest {
            request: 5,
            key: tile.key
        }
    );
}

#[test]
fn forest_descriptor_wire_roundtrip_material_coverage_and_allocation_guards() {
    let catalog = crate::content::catalog();
    let mut tile = LodTile {
        key: TileKey {
            level: 0,
            x: -1,
            z: -1,
        },
        revision: 31,
        geometric_error: 0,
        columns: vec![
            Column {
                coverage: vec![Interval {
                    bottom: -64,
                    top: 128
                }],
                spans: vec![]
            };
            1024
        ],
        trees: vec![TreeFeature {
            anchor: [-16, 12, -16],
            trunk_height: 9,
            support_y: Some(13),
            log: crate::world::WOOD,
            leaves: crate::world::LEAVES,
            branch_x: crate::world::WOOD,
            branch_z: crate::world::WOOD,
            species: 0,
            shape: 19,
        }],
    };
    let mut bytes = Vec::new();
    write_tile(&mut bytes, &tile, catalog).unwrap();
    assert_eq!(bytes.len(), tile_len(&tile));
    let mut cursor = Cursor {
        bytes: &bytes,
        offset: 0,
    };
    assert_eq!(read_tile(&mut cursor, catalog).unwrap(), tile);
    let mut omitted = tile.clone();
    omitted.trees[0].support_y = None;
    let mut omitted_bytes = Vec::new();
    write_tile(&mut omitted_bytes, &omitted, catalog).unwrap();
    assert_eq!(
        read_tile(
            &mut Cursor {
                bytes: &omitted_bytes,
                offset: 0
            },
            catalog
        )
        .unwrap(),
        omitted,
        "omission retains exact source descriptor through the wire"
    );
    let count = bytes.len() - TreeFeature::WIRE_BYTES - 2;
    bytes[count..count + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(
        read_tile(
            &mut Cursor {
                bytes: &bytes,
                offset: 0
            },
            catalog
        )
        .is_err(),
        "count bounded before allocation"
    );
    let mut excessive = Vec::new();
    write_tile(&mut excessive, &tile, catalog).unwrap();
    excessive[count..count + 2].copy_from_slice(&(MAX_TREE_FEATURES as u16).to_le_bytes());
    let error = read_tile(
        &mut Cursor {
            bytes: &excessive,
            offset: 0,
        },
        catalog,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("allocation cap"),
        "feature bytes rejected before reading/allocating omitted descriptors: {error}"
    );
    // Counts may individually fit their limits but jointly exceed the
    // unchanged wire budget. Stop at the offending counts: no span payload
    // exists, so rejection must precede allocation/reading that group.
    let mut excessive = Vec::new();
    write_key(&mut excessive, tile.key).unwrap();
    excessive.extend(0u64.to_le_bytes());
    excessive.extend(0u32.to_le_bytes());
    excessive.extend(1024u16.to_le_bytes());
    excessive.extend(1u16.to_le_bytes());
    excessive.push(1);
    excessive.extend(crate::world::STONE.get().to_le_bytes());
    for column in 0..1024 {
        excessive.extend(2u16.to_le_bytes());
        excessive.extend(4u16.to_le_bytes());
        if palette::HEADER_BYTES + 4 + 4 * 1024 + (column + 1) * (2 * 8 + 4 * 10) > MAX_TILE_BYTES {
            break;
        }
        for (bottom, top) in [(-64i32, 0i32), (1, 128)] {
            excessive.extend(bottom.to_le_bytes());
            excessive.extend(top.to_le_bytes());
        }
        for bottom in [-20i32, -18, -16, -14] {
            excessive.extend(bottom.to_le_bytes());
            excessive.extend((bottom + 1).to_le_bytes());
            excessive.extend([0, 0]);
        }
    }
    let error = read_tile(
        &mut Cursor {
            bytes: &excessive,
            offset: 0,
        },
        catalog,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("allocation cap"),
        "cumulative coverage/span bytes rejected before reading/allocating omitted group: {error}"
    );
    tile.trees[0].leaves = crate::world::STONE;
    assert!(
        tile.validate(catalog).is_err(),
        "opaque material cannotpretendleaf"
    );
    tile.trees[0].leaves = crate::world::LEAVES;
    tile.columns[16 + 32 * 16].coverage.clear();
    assert!(
        tile.validate(catalog).is_err(),
        "proxy cannotcross unknown interval"
    );
}

mod compact;
