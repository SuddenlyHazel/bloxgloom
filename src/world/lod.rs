//! Server-only builtin coarse sampling. Registered contributors use snapshot extraction.
//! A coarse cell represents its center column; saved deltas anywhere in that cell
//! override the approximation. Coverage describes the authoritative builtin domain,
//! not a claim that every fine voxel was enumerated.
mod snapshots;

use super::terrain::LodSampler;
use crate::{
    content::{CUTOUT, Catalog, OPAQUE, PLANT},
    lod::{Column, Interval, LodTile, Span, TILE_COLUMNS, TILE_SIZE, TileKey},
    world::{AIR, BEDROCK_Y, BlockId, CHUNK_SIZE, Chunk, MAX_GENERATED_HEIGHT, World},
};
use std::collections::BTreeMap;

pub(crate) fn builtin_lod_tile(
    key: TileKey,
    revision: u64,
    seed: u64,
    catalog: &Catalog,
) -> Result<LodTile, String> {
    build(key, revision, seed, catalog, &[])
}
impl World {
    pub(crate) fn lod_max_level(&self) -> u8 {
        if self.generator.is_builtin() { 4 } else { 3 }
    }
    pub(crate) fn lod_builtin_summary(
        &self,
        key: TileKey,
        revision: u64,
        overlays: &[Chunk],
    ) -> Option<Result<LodTile, String>> {
        self.generator
            .is_builtin()
            .then(|| build(key, revision, self.seed, self.catalog(), overlays))
    }
}
fn build(
    key: TileKey,
    revision: u64,
    seed: u64,
    catalog: &Catalog,
    overlays: &[Chunk],
) -> Result<LodTile, String> {
    if key.level > 5 || overlays.len() > 2048 {
        return Err("builtin LOD work budget exceeded".into());
    }
    let [minx, minz, maxx, maxz] = key.bounds().ok_or("invalid builtin LOD key")?;
    let width = key.sample_width().ok_or("invalid builtin LOD level")?;
    let bottom = BEDROCK_Y;
    let top = ((MAX_GENERATED_HEIGHT / CHUNK_SIZE as i32) + 1) * CHUNK_SIZE as i32;
    let mut sampler = LodSampler::new(seed);
    let mut cells: Vec<BTreeMap<i32, BlockId>> = Vec::with_capacity(TILE_COLUMNS);
    let mut coverages = vec![vec![Interval { bottom, top }]; TILE_COLUMNS];
    for z in 0..TILE_SIZE {
        for x in 0..TILE_SIZE {
            let wx = minx + x as i32 * width + width / 2;
            let wz = minz + z as i32 * width + width / 2;
            cells.push(
                (bottom..top)
                    .zip(sampler.column(i64::from(wx), i64::from(wz), bottom, top))
                    .collect(),
            );
        }
    }
    let mut overlays: Vec<_> = overlays.iter().collect();
    overlays.sort_by_key(|c| c.key);
    for c in overlays {
        if c.blocks.len() != CHUNK_SIZE.pow(3) {
            return Err("invalid builtin LOD overlay".into());
        }
        let cx = c
            .key
            .x
            .checked_mul(CHUNK_SIZE as i32)
            .ok_or("overlay coordinate overflow")?;
        let cz = c
            .key
            .z
            .checked_mul(CHUNK_SIZE as i32)
            .ok_or("overlay coordinate overflow")?;
        let cy = c
            .key
            .y
            .checked_mul(CHUNK_SIZE as i32)
            .ok_or("overlay coordinate overflow")?;
        let ct = cy
            .checked_add(CHUNK_SIZE as i32)
            .ok_or("overlay coordinate overflow")?;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = cx
                    .checked_add(x as i32)
                    .ok_or("overlay coordinate overflow")?;
                let wz = cz
                    .checked_add(z as i32)
                    .ok_or("overlay coordinate overflow")?;
                if wx < minx || wx >= maxx || wz < minz || wz >= maxz {
                    continue;
                }
                let index =
                    ((wx - minx) / width) as usize + ((wz - minz) / width) as usize * TILE_SIZE;
                coverages[index].push(Interval {
                    bottom: cy,
                    top: ct,
                });
                let baseline = sampler.column(i64::from(wx), i64::from(wz), cy, ct);
                for (y, original) in baseline.into_iter().enumerate() {
                    let id = c.block([x, y, z]).ok_or("invalid overlay voxel")?;
                    if catalog.state(id).is_none() {
                        return Err("unknown overlay state".into());
                    }
                    let wy = cy + y as i32;
                    if wy < bottom {
                        cells[index].entry(wy).or_insert(original);
                    }
                    if id != original {
                        cells[index].insert(wy, id);
                    }
                }
            }
        }
    }
    let mut columns = Vec::with_capacity(TILE_COLUMNS);
    for (states, mut coverage) in cells.into_iter().zip(coverages) {
        coverage.sort_by_key(|c| c.bottom);
        let mut joined: Vec<Interval> = Vec::new();
        for c in coverage {
            if let Some(last) = joined.last_mut()
                && last.top >= c.bottom
            {
                last.top = last.top.max(c.top);
            } else {
                joined.push(c);
            }
        }
        let mut spans: Vec<Span> = Vec::new();
        for (y, id) in states {
            let state = catalog.state(id).ok_or("unknown builtin state")?;
            if id == AIR || state.flags & (OPAQUE | CUTOUT) == 0 || state.flags & PLANT != 0 {
                continue;
            }
            if let Some(last) = spans.last_mut()
                && last.top == y
                && last.state == id
                && last.glow == state.emission
            {
                last.top = y + 1;
            } else {
                spans.push(Span {
                    bottom: y,
                    top: y + 1,
                    state: id,
                    sky: 0,
                    glow: state.emission,
                });
            }
        }
        let mut column = Column {
            coverage: joined,
            spans,
        };
        crate::lod::skylight::assign(&mut column, catalog);
        columns.push(column);
    }
    let tile = LodTile {
        key,
        revision,
        columns,
        // Center sampling can miss a cliff or an underground void. Horizontal
        // sample spacing alone is not a bound on that vertical displacement;
        // use the full finite builtin coverage height until tighter generator
        // error bounds are measured. Level zero samples every column exactly.
        geometric_error: if key.level == 0 {
            0
        } else {
            (width as u32 - 1).max((top - bottom) as u32)
        },
    };
    tile.into_render_summary(catalog)
}

#[cfg(test)]
mod tests;
