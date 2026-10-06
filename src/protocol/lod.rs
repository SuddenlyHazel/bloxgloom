//! Strictly bounded distant terrain wire encoding, also used for disposable caches.
use super::{Cursor, invalid};
use crate::content::{BlockStateId, Catalog};
use crate::lod::palette;
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
    tile.encoded_bytes()
}
pub(super) fn write_tile(out: &mut Vec<u8>, tile: &LodTile, catalog: &Catalog) -> io::Result<()> {
    tile.validate(catalog)
        .map_err(|_| invalid("invalid LOD summary"))?;
    out.try_reserve_exact(tile_len(tile))
        .map_err(|_| invalid("LOD allocation failed"))?;
    write_key(out, tile.key)?;
    out.extend(tile.revision.to_le_bytes());
    out.extend(tile.geometric_error.to_le_bytes());
    out.extend((tile.columns.len() as u16).to_le_bytes());
    let states = palette::states(tile);
    let width = palette::index_width(states.len());
    out.extend((states.len() as u16).to_le_bytes());
    out.push(width as u8);
    for state in &states {
        out.extend(state.0.to_le_bytes());
    }
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
            let index = states.binary_search(&v.state).unwrap();
            if width == 1 {
                out.push(index as u8);
            } else {
                out.extend((index as u16).to_le_bytes());
            }
            out.push(v.sky | (v.glow << 4));
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
    let palette_count = c.u16()? as usize;
    let width = usize::from(c.u8()?);
    let base_bytes = palette::HEADER_BYTES + 4 * count + 4 * palette_count;
    if palette_count > MAX_TILE_SPANS
        || width != palette::index_width(palette_count)
        || base_bytes > MAX_TILE_BYTES
    {
        return Err(invalid("invalid LOD palette count or width"));
    }
    // Check actual bytes before reserve: counts alone never authorize allocation.
    let mut encoded_palette = Cursor {
        bytes: c.take(palette_count * 4)?,
        offset: 0,
    };
    let mut states = bounded_vec(palette_count)?;
    for _ in 0..palette_count {
        let state = BlockStateId(encoded_palette.u32()?);
        let flags = catalog
            .state(state)
            .ok_or_else(|| invalid("unknown LOD palette state"))?
            .flags;
        if flags & (crate::content::OPAQUE | crate::content::CUTOUT | crate::content::FLUID) == 0
            || flags & crate::content::PLANT != 0
            || states.last().is_some_and(|previous| *previous >= state)
        {
            return Err(invalid("invalid or unordered LOD palette state"));
        }
        states.push(state);
    }
    let mut used = bounded_vec(palette_count)?;
    used.resize(palette_count, false);
    let mut columns = bounded_vec(count)?;
    let span_bytes = 9 + width;
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
            || base_bytes + total_coverage * 8 + total_spans * span_bytes > MAX_TILE_BYTES
        {
            return Err(invalid("LOD allocation cap exceeded"));
        }
        let mut payload = Cursor {
            bytes: c.take(coverage_count * 8 + span_count * span_bytes)?,
            offset: 0,
        };
        let mut coverage = bounded_vec(coverage_count)?;
        let mut spans = bounded_vec(span_count)?;
        for _ in 0..coverage_count {
            coverage.push(Interval {
                bottom: payload.i32()?,
                top: payload.i32()?,
            });
        }
        for _ in 0..span_count {
            let bottom = payload.i32()?;
            let top = payload.i32()?;
            let index = if width == 1 {
                usize::from(payload.u8()?)
            } else {
                usize::from(payload.u16()?)
            };
            let state = *states
                .get(index)
                .ok_or_else(|| invalid("invalid LOD palette index"))?;
            used[index] = true;
            let light = payload.u8()?;
            spans.push(Span {
                bottom,
                top,
                state,
                sky: light & 15,
                glow: light >> 4,
            });
        }
        columns.push(Column { coverage, spans });
    }
    let count = c.u16()? as usize;
    if count > MAX_TREE_FEATURES
        || base_bytes
            + total_coverage * 8
            + total_spans * span_bytes
            + count * TreeFeature::WIRE_BYTES
            > MAX_TILE_BYTES
    {
        return Err(invalid("forest allocation cap exceeded"));
    }
    let [x, z, _, _] = key
        .bounds()
        .ok_or_else(|| invalid("invalid forest bounds"))?;
    if used.iter().any(|used| !used) {
        return Err(invalid("unused LOD palette state"));
    }
    let mut tree_payload = Cursor {
        bytes: c.take(count * TreeFeature::WIRE_BYTES)?,
        offset: 0,
    };
    let c = &mut tree_payload;
    let mut trees = bounded_vec(count)?;
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

fn bounded_vec<T>(count: usize) -> io::Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| invalid("LOD allocation failed"))?;
    Ok(values)
}

#[cfg(test)]
mod tests;
