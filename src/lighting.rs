//! Bounded voxel light propagation around a chunk. Light is derived from block
//! snapshots, not saved separately, so terrain edits can rebuild it safely.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::content::{self, Catalog};
use crate::world::{self, BlockId, CHUNK_SIZE, Chunk, ChunkKey, STONE};

const SIDE: usize = CHUNK_SIZE * 3;
const PLANE: usize = SIDE * SIDE;
const VOLUME: usize = SIDE * SIDE * SIDE;
const MAX_LIGHT: u8 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightSample {
    pub sky: u8,
    pub glow: u8,
    pub bounce: [u8; 3],
}

pub struct LightField {
    sky: Vec<u8>,
    glow: Vec<u8>,
    bounce: Option<Vec<[u8; 3]>>,
}

impl LightField {
    /// The one-chunk halo is wider than the 15-step propagation range. Chunks
    /// not yet streamed use the deterministic baseline until their snapshot
    /// arrives, at which point the client re-lights affected neighbors.
    #[cfg(test)]
    pub fn build(key: ChunkKey, known: &HashMap<ChunkKey, Arc<Chunk>>, seed: u64) -> Self {
        Self::build_with_catalog(key, known, seed, content::catalog())
    }

    pub fn build_with_catalog(
        key: ChunkKey,
        known: &HashMap<ChunkKey, Arc<Chunk>>,
        seed: u64,
        catalog: &Catalog,
    ) -> Self {
        Self::build_with_bounce_and_catalog(key, known, seed, false, catalog)
    }

    pub fn build_with_bounce(
        key: ChunkKey,
        known: &HashMap<ChunkKey, Arc<Chunk>>,
        seed: u64,
        bounced: bool,
    ) -> Self {
        Self::build_with_bounce_and_catalog(key, known, seed, bounced, content::catalog())
    }

    pub fn build_with_bounce_and_catalog(
        key: ChunkKey,
        known: &HashMap<ChunkKey, Arc<Chunk>>,
        seed: u64,
        bounced: bool,
        catalog: &Catalog,
    ) -> Self {
        let mut blocks = vec![STONE; VOLUME];
        for cy in 0..3 {
            for cz in 0..3 {
                for cx in 0..3 {
                    let Some(chunk_key) =
                        key_offset(key, cx as i32 - 1, cy as i32 - 1, cz as i32 - 1)
                    else {
                        continue;
                    };
                    let fallback;
                    let chunk = if let Some(chunk) = known.get(&chunk_key) {
                        chunk.as_ref()
                    } else {
                        fallback = world::generate_chunk(chunk_key, seed);
                        &fallback
                    };
                    for y in 0..CHUNK_SIZE {
                        for z in 0..CHUNK_SIZE {
                            let source = Chunk::index([0, y, z]).unwrap();
                            let target =
                                index(cx * CHUNK_SIZE, cy * CHUNK_SIZE + y, cz * CHUNK_SIZE + z);
                            chunk
                                .blocks
                                .copy_range_to(
                                    source..source + CHUNK_SIZE,
                                    &mut blocks[target..target + CHUNK_SIZE],
                                )
                                .expect("complete chunk row");
                        }
                    }
                }
            }
        }
        let mut sky = vec![0; VOLUME];
        let mut glow = vec![0; VOLUME];
        let mut sky_frontier = VecDeque::new();
        let mut glow_frontier = VecDeque::new();
        let top_world_y = (i64::from(key.y) + 2) * CHUNK_SIZE as i64 - 1;
        let first_x = (i64::from(key.x) - 1) * CHUNK_SIZE as i64;
        let first_z = (i64::from(key.z) - 1) * CHUNK_SIZE as i64;
        for z in 0..SIDE {
            for x in 0..SIDE {
                let mut open_to_sky =
                    world::terrain_height(first_x + x as i64, first_z + z as i64, seed)
                        <= top_world_y;
                for y in (0..SIDE).rev() {
                    let at = index(x, y, z);
                    let emission = catalog.emission(blocks[at]);
                    if emission != 0 {
                        glow[at] = emission;
                        glow_frontier.push_back(at);
                    }
                    if is_opaque(catalog, blocks[at]) {
                        open_to_sky = false;
                    } else if open_to_sky {
                        sky[at] = MAX_LIGHT;
                        sky_frontier.push_back(at);
                    }
                }
            }
        }
        propagate(&blocks, &mut sky, sky_frontier, catalog);
        propagate(&blocks, &mut glow, glow_frontier, catalog);
        let bounce = bounced.then(|| build_bounce(&blocks, &sky, &glow, catalog));
        Self { sky, glow, bounce }
    }

    pub fn face(&self, local: [usize; 3], axis: usize, side: i32) -> LightSample {
        let mut point = [
            local[0] + CHUNK_SIZE,
            local[1] + CHUNK_SIZE,
            local[2] + CHUNK_SIZE,
        ];
        point[axis] = point[axis].checked_add_signed(side as isize).unwrap();
        let at = index(point[0], point[1], point[2]);
        LightSample {
            sky: self.sky[at],
            glow: self.glow[at],
            bounce: self.bounce.as_ref().map_or([0; 3], |bounce| bounce[at]),
        }
    }

    /// Average the four air-side voxels around a face corner. Opaque neighbors
    /// contribute zero, giving a cheap corner-occlusion term without a new pass.
    pub fn corner(
        &self,
        axes: [usize; 3],
        side: i32,
        slice: usize,
        corner: [usize; 2],
    ) -> [f32; 5] {
        let [axis, u, v] = axes;
        let [corner_u, corner_v] = corner;
        let mut sky = 0u32;
        let mut glow = 0u32;
        let mut bounce = [0u32; 3];
        for du in [-1isize, 0] {
            for dv in [-1isize, 0] {
                let mut point = [CHUNK_SIZE; 3];
                point[axis] = (CHUNK_SIZE + slice)
                    .checked_add_signed(if side > 0 { 1 } else { -1 })
                    .unwrap();
                point[u] = (CHUNK_SIZE + corner_u).checked_add_signed(du).unwrap();
                point[v] = (CHUNK_SIZE + corner_v).checked_add_signed(dv).unwrap();
                let at = index(point[0], point[1], point[2]);
                sky += u32::from(self.sky[at]);
                glow += u32::from(self.glow[at]);
                if let Some(field) = &self.bounce {
                    for channel in 0..3 {
                        bounce[channel] += u32::from(field[at][channel]);
                    }
                }
            }
        }
        [
            sky as f32 / 60.0,
            glow as f32 / 60.0,
            bounce[0] as f32 / 1020.0,
            bounce[1] as f32 / 1020.0,
            bounce[2] as f32 / 1020.0,
        ]
    }
}

/// One diffuse reflection from opaque surfaces. Sources are derived only from
/// direct/propagated sky and emission; bounced light cannot bounce again.
fn build_bounce(blocks: &[BlockId], sky: &[u8], glow: &[u8], catalog: &Catalog) -> Vec<[u8; 3]> {
    let mut bounce = vec![[0u8; 3]; VOLUME];
    let mut frontier = VecDeque::new();
    for y in 1..SIDE - 1 {
        for z in 1..SIDE - 1 {
            for x in 1..SIDE - 1 {
                let at = index(x, y, z);
                if is_opaque(catalog, blocks[at]) {
                    continue;
                }
                // Keep reflected energy well below incident energy, even where
                // multiple faces meet. Max avoids corner over-brightening.
                if sky[at] == 0 && glow[at] == 0 {
                    continue;
                }
                for neighbor in [at - 1, at + 1, at - SIDE, at + SIDE, at - PLANE, at + PLANE] {
                    if !is_opaque(catalog, blocks[neighbor]) {
                        continue;
                    }
                    let reflectance = catalog.reflectance(blocks[neighbor]);
                    for channel in 0..3 {
                        let sky_color = [82u16, 105, 145][channel];
                        let glow_color = [205u16, 125, 65][channel];
                        let incident = ((u16::from(sky[at]) * sky_color
                            + u16::from(glow[at]) * glow_color)
                            / 15)
                            .min(255);
                        bounce[at][channel] = bounce[at][channel]
                            .max(((incident * u16::from(reflectance[channel])) / 255) as u8);
                    }
                }
                if bounce[at] != [0; 3] {
                    frontier.push_back(at);
                }
            }
        }
    }
    // RGB max-propagation attenuates in air and never creates another source.
    while let Some(at) = frontier.pop_front() {
        let next = bounce[at].map(|channel| channel.saturating_sub(18));
        if next == [0; 3] {
            continue;
        }
        let x = at % SIDE;
        let z = at / SIDE % SIDE;
        let y = at / PLANE;
        for neighbor in [
            (x > 0).then_some(at.wrapping_sub(1)),
            (x + 1 < SIDE).then_some(at + 1),
            (z > 0).then_some(at.wrapping_sub(SIDE)),
            (z + 1 < SIDE).then_some(at + SIDE),
            (y > 0).then_some(at.wrapping_sub(PLANE)),
            (y + 1 < SIDE).then_some(at + PLANE),
        ]
        .into_iter()
        .flatten()
        {
            if is_opaque(catalog, blocks[neighbor]) {
                continue;
            }
            let mut changed = false;
            for channel in 0..3 {
                if next[channel] > bounce[neighbor][channel] {
                    bounce[neighbor][channel] = next[channel];
                    changed = true;
                }
            }
            if changed {
                frontier.push_back(neighbor);
            }
        }
    }
    bounce
}

#[inline]
fn is_opaque(catalog: &Catalog, block: BlockId) -> bool {
    catalog.block_flags(block) & content::OPAQUE != 0
}

fn key_offset(key: ChunkKey, x: i32, y: i32, z: i32) -> Option<ChunkKey> {
    Some(ChunkKey {
        x: key.x.checked_add(x)?,
        y: key.y.checked_add(y)?,
        z: key.z.checked_add(z)?,
    })
}

#[inline]
fn index(x: usize, y: usize, z: usize) -> usize {
    x + SIDE * z + PLANE * y
}

fn propagate(
    blocks: &[BlockId],
    light: &mut [u8],
    mut frontier: VecDeque<usize>,
    catalog: &Catalog,
) {
    while let Some(at) = frontier.pop_front() {
        let next = light[at].saturating_sub(1);
        if next == 0 {
            continue;
        }
        let x = at % SIDE;
        let z = at / SIDE % SIDE;
        let y = at / PLANE;
        for neighbor in [
            (x > 0).then_some(at.wrapping_sub(1)),
            (x + 1 < SIDE).then_some(at + 1),
            (z > 0).then_some(at.wrapping_sub(SIDE)),
            (z + 1 < SIDE).then_some(at + SIDE),
            (y > 0).then_some(at.wrapping_sub(PLANE)),
            (y + 1 < SIDE).then_some(at + PLANE),
        ]
        .into_iter()
        .flatten()
        {
            if !is_opaque(catalog, blocks[neighbor]) && light[neighbor] < next {
                light[neighbor] = next;
                frontier.push_back(neighbor);
            }
        }
    }
}

#[cfg(test)]
#[path = "lighting/tests.rs"]
mod tests;
