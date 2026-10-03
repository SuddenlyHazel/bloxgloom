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

mod local;
mod sampling;
mod skylight;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightSample {
    pub sky: u8,
    pub glow: u8,
    /// Normalized dominant emitter tint, derived from catalog reflectance.
    pub glow_color: [u8; 3],
    /// Direction toward incoming transport, scaled by 127; length is confidence.
    pub glow_direction: [i8; 3],
    pub bounce: [u8; 3],
    /// Emission-only reflection, retained when sunlight fades.
    pub glow_bounce: [u8; 3],
}

pub struct LightField {
    sky: Vec<u8>,
    glow: Vec<u8>,
    local: Option<Vec<local::LocalLight>>,
    bounce: Option<Vec<[u8; 3]>>,
    glow_bounce: Option<Vec<[u8; 3]>>,
}

impl LightField {
    /// The one-chunk halo covers local propagation; direct sky additionally
    /// checks columns above it. Missing chunks use the deterministic baseline
    /// until streamed snapshots arrive and invalidate dependent lighting.
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
        let incoming_sky = skylight::incoming(key, known, seed, catalog, &blocks);
        for z in 0..SIDE {
            for x in 0..SIDE {
                let mut direct_sky = incoming_sky[z * SIDE + x];
                for y in (0..SIDE).rev() {
                    let at = index(x, y, z);
                    let emission = catalog.emission(blocks[at]);
                    if emission != 0 {
                        glow[at] = emission;
                        glow_frontier.push_back(at);
                    }
                    direct_sky = direct_sky.saturating_sub(catalog.sky_attenuation(blocks[at]));
                    if direct_sky != 0 {
                        sky[at] = direct_sky;
                        sky_frontier.push_back(at);
                    }
                }
            }
        }
        propagate(&blocks, &mut sky, sky_frontier, catalog, true);
        propagate(&blocks, &mut glow, glow_frontier, catalog, false);
        let local = local::build(&blocks, &glow, catalog);
        let bounce = bounced.then(|| build_bounce(&blocks, &sky, &glow, local.as_deref(), catalog));
        let glow_bounce = (bounced && glow.iter().any(|&value| value != 0))
            .then(|| build_bounce(&blocks, &vec![0; VOLUME], &glow, local.as_deref(), catalog));
        Self {
            sky,
            glow,
            local,
            bounce,
            glow_bounce,
        }
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
            glow_color: self.local.as_ref().map_or([0; 3], |field| field[at].color),
            glow_direction: self
                .local
                .as_ref()
                .map_or([0; 3], |field| field[at].direction),
            bounce: self.bounce.as_ref().map_or([0; 3], |bounce| bounce[at]),
            glow_bounce: self
                .glow_bounce
                .as_ref()
                .map_or([0; 3], |bounce| bounce[at]),
        }
    }

    /// Average the four air-side voxels around a face corner. Opaque neighbors
    /// contribute zero, giving a cheap corner-occlusion term without a new pass.
    #[cfg(test)]
    pub fn corner(
        &self,
        axes: [usize; 3],
        side: i32,
        slice: usize,
        corner: [usize; 2],
    ) -> [f32; 8] {
        self.corner_with_visibility(axes, side, slice, corner).0
    }

    /// Alongside the unchanged averaged irradiance, expose its conservative
    /// local averaging visibility (not exact geometric occupancy). Dark neighbors
    /// and irradiance gradients are already counted by the average;
    /// screen-space AO must union with that suppression rather than multiply it.
    pub(crate) fn corner_with_visibility(
        &self,
        axes: [usize; 3],
        side: i32,
        slice: usize,
        corner: [usize; 2],
    ) -> ([f32; 8], f32) {
        let [axis, u, v] = axes;
        let [corner_u, corner_v] = corner;
        let mut sky = 0u32;
        let mut maxima = [0u32; 7];
        let mut glow = 0u32;
        let mut bounce = [0u32; 3];
        let mut glow_bounce = [0u32; 3];
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
                maxima[0] = maxima[0].max(u32::from(self.sky[at]));
                glow += u32::from(self.glow[at]);
                if let Some(field) = &self.glow_bounce {
                    for channel in 0..3 {
                        glow_bounce[channel] += u32::from(field[at][channel]);
                        maxima[4 + channel] =
                            maxima[4 + channel].max(u32::from(field[at][channel]));
                    }
                }
                if let Some(field) = &self.bounce {
                    for channel in 0..3 {
                        bounce[channel] += u32::from(field[at][channel]);
                        maxima[1 + channel] =
                            maxima[1 + channel].max(u32::from(field[at][channel]));
                    }
                }
            }
        }
        let sums = [
            sky,
            bounce[0],
            bounce[1],
            bounce[2],
            glow_bounce[0],
            glow_bounce[1],
            glow_bounce[2],
        ];
        let visibility = sums
            .into_iter()
            .zip(maxima)
            .filter(|(_, max)| *max > 0)
            .map(|(sum, max)| sum as f32 / (4.0 * max as f32))
            .fold(1.0f32, f32::min);
        (
            [
                sky as f32 / 60.0,
                glow as f32 / 60.0,
                bounce[0] as f32 / 1020.0,
                bounce[1] as f32 / 1020.0,
                bounce[2] as f32 / 1020.0,
                glow_bounce[0] as f32 / 1020.0,
                glow_bounce[1] as f32 / 1020.0,
                glow_bounce[2] as f32 / 1020.0,
            ],
            visibility,
        )
    }
}

/// One diffuse reflection from opaque surfaces. Sources are derived only from
/// direct/propagated sky and emission; bounced light cannot bounce again.
fn build_bounce(
    blocks: &[BlockId],
    sky: &[u8],
    glow: &[u8],
    local: Option<&[local::LocalLight]>,
    catalog: &Catalog,
) -> Vec<[u8; 3]> {
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
                        let glow_color = local
                            .map_or(0, |field| u16::from(field[at].color[channel]) * 205 / 255);
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
    skylight: bool,
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
            // Diffuse light loses at least one level in air; absorption through
            // foliage must also apply laterally so propagation cannot bypass it.
            let transmitted = if skylight {
                light[at].saturating_sub(catalog.sky_attenuation(blocks[neighbor]).max(1))
            } else {
                next
            };
            if !is_opaque(catalog, blocks[neighbor]) && light[neighbor] < transmitted {
                light[neighbor] = transmitted;
                frontier.push_back(neighbor);
            }
        }
    }
}

#[cfg(test)]
#[path = "lighting/tests.rs"]
mod tests;
