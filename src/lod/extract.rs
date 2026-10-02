//! Immutable snapshots only: no procedural estimates or world cache mutation.
use super::{Column, Interval, LodTile, Span, TILE_COLUMNS, TILE_SIZE, TileKey, reduce::merge};
use crate::{
    content::{CUTOUT, Catalog, OPAQUE, PLANT},
    world::{CHUNK_SIZE, Chunk},
};
use std::collections::BTreeMap;

pub fn extract(
    key: TileKey,
    revision: u64,
    chunks: &[Chunk],
    catalog: &Catalog,
) -> Result<LodTile, String> {
    let [minx, minz, maxx, maxz] = key.bounds().ok_or("invalid LOD tile coordinates")?;
    let width = key.sample_width().ok_or("invalid LOD level")?;
    // These are independent of render payload caps: accurate source columns
    // can be larger, but malformed or excessively tall requests stay bounded.
    const MAX_SOURCE_COLUMNS: usize = 262_144;
    const MAX_SOURCE_SPANS: usize = 1_048_576;
    const MAX_SOURCE_INTERVALS: usize = 1_048_576;
    if chunks.len() > 32_768 {
        return Err("LOD source snapshot budget exceeded".into());
    }
    let mut source_spans = 0;
    let mut source_intervals = 0;
    let mut fine: BTreeMap<(i32, i32), Column> = BTreeMap::new();
    for chunk in chunks {
        if chunk.blocks.len() != CHUNK_SIZE.pow(3) {
            return Err("invalid source chunk size".into());
        }
        let cx = chunk
            .key
            .x
            .checked_mul(CHUNK_SIZE as i32)
            .ok_or("source coordinate overflow")?;
        let cy = chunk
            .key
            .y
            .checked_mul(CHUNK_SIZE as i32)
            .ok_or("source coordinate overflow")?;
        let cz = chunk
            .key
            .z
            .checked_mul(CHUNK_SIZE as i32)
            .ok_or("source coordinate overflow")?;
        let top = cy
            .checked_add(CHUNK_SIZE as i32)
            .ok_or("source coordinate overflow")?;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = cx
                    .checked_add(x as i32)
                    .ok_or("source coordinate overflow")?;
                let wz = cz
                    .checked_add(z as i32)
                    .ok_or("source coordinate overflow")?;
                if wx < minx || wx >= maxx || wz < minz || wz >= maxz {
                    continue;
                }
                if fine.len() >= MAX_SOURCE_COLUMNS && !fine.contains_key(&(wx, wz)) {
                    return Err("LOD source column budget exceeded".into());
                }
                let column = fine.entry((wx, wz)).or_default();
                if column.coverage.iter().any(|v| v.bottom < top && v.top > cy) {
                    return Err("duplicate source coverage".into());
                }
                column.coverage.push(Interval { bottom: cy, top });
                source_intervals += 1;
                if source_intervals > MAX_SOURCE_INTERVALS {
                    return Err("LOD source coverage budget exceeded".into());
                }
                for y in 0..CHUNK_SIZE {
                    let id = chunk.block([x, y, z]).ok_or("missing source block")?;
                    let state = catalog.state(id).ok_or("unknown source state")?;
                    if state.flags & (OPAQUE | CUTOUT) == 0 || state.flags & PLANT != 0 {
                        continue;
                    }
                    let span = Span {
                        bottom: cy + y as i32,
                        top: cy + y as i32 + 1,
                        state: id,
                        sky: 0,
                        glow: state.emission.min(15),
                    };
                    if let Some(last) = column.spans.last_mut()
                        && last.top == span.bottom
                        && last.state == span.state
                        && last.glow == span.glow
                    {
                        last.top = span.top;
                    } else {
                        source_spans += 1;
                        if source_spans > MAX_SOURCE_SPANS {
                            return Err("LOD source span budget exceeded".into());
                        }
                        column.spans.push(span);
                    }
                }
            }
        }
    }
    for c in fine.values_mut() {
        c.coverage.sort_by_key(|v| v.bottom);
        let mut joined: Vec<Interval> = Vec::new();
        for v in &c.coverage {
            if let Some(last) = joined.last_mut()
                && last.top == v.bottom
            {
                last.top = v.top;
            } else {
                joined.push(*v);
            }
        }
        c.coverage = joined;
        c.spans.sort_by_key(|s| s.bottom);
        let mut joined: Vec<Span> = Vec::new();
        for s in &c.spans {
            if let Some(last) = joined.last_mut()
                && last.top == s.bottom
                && last.state == s.state
                && last.glow == s.glow
            {
                last.top = s.top;
            } else {
                joined.push(*s);
            }
        }
        c.spans = joined;
        // Only the highest surface beneath observed air receives approximate sky.
        // A coverage gap above it remains unknown and therefore dark.
        if let Some(last) = c.spans.last_mut()
            && c.coverage
                .last()
                .is_some_and(|v| v.bottom <= last.top && v.top > last.top)
        {
            last.sky = 15;
        }
    }
    let mut groups: Vec<Vec<&Column>> = vec![Vec::new(); TILE_COLUMNS];
    for ((x, z), c) in &fine {
        let xx = (x - minx) / width;
        let zz = (z - minz) / width;
        groups[xx as usize + zz as usize * TILE_SIZE].push(c);
    }
    let expected = (width as u64) * (width as u64);
    let columns = groups
        .into_iter()
        .map(|g| {
            if g.len() as u64 == expected {
                merge(&g)
            } else {
                Column::default()
            }
        })
        .collect();
    let tile = LodTile {
        key,
        revision,
        columns,
        geometric_error: width as u32 - 1,
    };
    tile.validate(catalog)?;
    Ok(tile)
}
