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
mod tests;
