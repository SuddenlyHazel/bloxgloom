//! Strictly bounded distant terrain wire encoding, also used for disposable caches.
use super::{Cursor, invalid};
use crate::content::{BlockStateId, Catalog};
use crate::lod::{
    Column, Interval, LodTile, MAX_TILE_BYTES, MAX_TILE_COVERAGE, MAX_TILE_SPANS,
    MAX_TREE_FEATURES, Span, TileKey, TreeFeature,
};
use std::io;

pub(super) fn write_key(out: &mut Vec<u8>, key: TileKey) -> io::Result<()> {
    if key.bounds().is_none() {
        return Err(invalid("invalid LOD tile coordinate"));
    }
    out.push(key.level);
    out.extend(key.x.to_le_bytes());
    out.extend(key.z.to_le_bytes());
    Ok(())
}
pub(super) fn read_key(c: &mut Cursor<'_>) -> io::Result<TileKey> {
    let key = TileKey {
        level: c.u8()?,
        x: c.i32()?,
        z: c.i32()?,
    };
    if key.bounds().is_none() {
        return Err(invalid("invalid LOD tile coordinate"));
    }
    Ok(key)
}
pub(crate) fn tile_len(tile: &LodTile) -> usize {
    9 + 8
        + 2
        + tile.trees.len() * TreeFeature::WIRE_BYTES
        + 4
        + 2
        + tile
            .columns
            .iter()
            .map(|col| 4 + col.coverage.len() * 8 + col.spans.len() * 14)
            .sum::<usize>()
}
pub(super) fn write_tile(out: &mut Vec<u8>, tile: &LodTile, catalog: &Catalog) -> io::Result<()> {
    tile.validate(catalog)
        .map_err(|_| invalid("invalid LOD summary"))?;
    write_key(out, tile.key)?;
    out.extend(tile.revision.to_le_bytes());
    out.extend(tile.geometric_error.to_le_bytes());
    out.extend((tile.columns.len() as u16).to_le_bytes());
    for col in &tile.columns {
        out.extend((col.coverage.len() as u16).to_le_bytes());
        out.extend((col.spans.len() as u16).to_le_bytes());
        for v in &col.coverage {
            out.extend(v.bottom.to_le_bytes());
            out.extend(v.top.to_le_bytes());
        }
        for v in &col.spans {
            out.extend(v.bottom.to_le_bytes());
            out.extend(v.top.to_le_bytes());
            out.extend(v.state.0.to_le_bytes());
            out.push(v.sky);
            out.push(v.glow);
        }
    }
    out.extend((tile.trees.len() as u16).to_le_bytes());
    let [x, z, _, _] = tile
        .key
        .bounds()
        .ok_or_else(|| invalid("invalid forest tile bounds"))?;
    for tree in &tile.trees {
        out.extend(((tree.anchor[0] - x) as i16).to_le_bytes());
        out.extend((tree.anchor[1] as i16).to_le_bytes());
        out.extend(((tree.anchor[2] - z) as i16).to_le_bytes());
        out.extend([tree.trunk_height, tree.species, tree.shape]);
        out.extend(tree.support_y.unwrap_or(i16::MIN).to_le_bytes());
        for state in [tree.log, tree.leaves, tree.branch_x, tree.branch_z] {
            out.extend(state.0.to_le_bytes());
        }
    }
    Ok(())
}
pub(super) fn read_tile(c: &mut Cursor<'_>, catalog: &Catalog) -> io::Result<LodTile> {
    let key = read_key(c)?;
    let revision = c.u64()?;
    let geometric_error = c.u32()?;
    let count = c.u16()? as usize;
    if count != 1024 {
        return Err(invalid("invalid LOD column count"));
    }
    let mut columns = Vec::with_capacity(count);
    let mut total_spans = 0usize;
    let mut total_coverage = 0usize;
    for _ in 0..count {
        let coverage_count = c.u16()? as usize;
        let span_count = c.u16()? as usize;
        total_spans += span_count;
        total_coverage += coverage_count;
        if span_count > 32
            || total_spans > MAX_TILE_SPANS
            || total_coverage > MAX_TILE_COVERAGE
            || 34 + 4 * 1024 + total_coverage * 8 + total_spans * 14 > MAX_TILE_BYTES
        {
            return Err(invalid("LOD allocation cap exceeded"));
        }
        let mut coverage = Vec::with_capacity(coverage_count);
        let mut spans = Vec::with_capacity(span_count);
        for _ in 0..coverage_count {
            coverage.push(Interval {
                bottom: c.i32()?,
                top: c.i32()?,
            });
        }
        for _ in 0..span_count {
            spans.push(Span {
                bottom: c.i32()?,
                top: c.i32()?,
                state: BlockStateId(c.u32()?),
                sky: c.u8()?,
                glow: c.u8()?,
            });
        }
        columns.push(Column { coverage, spans });
    }
    let count = c.u16()? as usize;
    if count > MAX_TREE_FEATURES
        || 34 + 4 * 1024 + total_coverage * 8 + total_spans * 14 + count * TreeFeature::WIRE_BYTES
            > MAX_TILE_BYTES
    {
        return Err(invalid("forest allocation cap exceeded"));
    }
    let [x, z, _, _] = key
        .bounds()
        .ok_or_else(|| invalid("invalid forest bounds"))?;
    let mut trees = Vec::with_capacity(count);
    for _ in 0..count {
        let dx = i32::from(c.i16()?);
        let y = i32::from(c.i16()?);
        let dz = i32::from(c.i16()?);
        trees.push(TreeFeature {
            anchor: [
                x.checked_add(dx)
                    .ok_or_else(|| invalid("forest x overflow"))?,
                y,
                z.checked_add(dz)
                    .ok_or_else(|| invalid("forest z overflow"))?,
            ],
            trunk_height: c.u8()?,
            species: c.u8()?,
            shape: c.u8()?,
            support_y: {
                let value = c.i16()?;
                (value != i16::MIN).then_some(value)
            },
            log: BlockStateId(c.u32()?),
            leaves: BlockStateId(c.u32()?),
            branch_x: BlockStateId(c.u32()?),
            branch_z: BlockStateId(c.u32()?),
        });
    }
    let tile = LodTile {
        trees,
        key,
        revision,
        columns,
        geometric_error,
    };
    tile.validate(catalog)
        .map_err(|_| invalid("invalid LOD summary"))?;
    Ok(tile)
}

#[cfg(test)]
mod tests {
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
        let span_count = 4 + 2 + 16 + 9 + 8 + 4 + 2 + 2;
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
}

#[cfg(test)]
mod forest_tests {
    use super::*;
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
        for column in 0..1024 {
            excessive.extend(2u16.to_le_bytes());
            excessive.extend(3u16.to_le_bytes());
            if 34 + 4 * 1024 + (column + 1) * (2 * 8 + 3 * 14) > MAX_TILE_BYTES {
                break;
            }
            for (bottom, top) in [(-64i32, 0i32), (1, 128)] {
                excessive.extend(bottom.to_le_bytes());
                excessive.extend(top.to_le_bytes());
            }
            for bottom in [-20i32, -18, -16] {
                excessive.extend(bottom.to_le_bytes());
                excessive.extend((bottom + 1).to_le_bytes());
                excessive.extend(crate::world::STONE.get().to_le_bytes());
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
}
