//! Bounded voxel light propagation around a chunk. Light is derived from block
//! snapshots, not saved separately, so terrain edits can rebuild it safely.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::world::{self, AIR, CHUNK_SIZE, Chunk, ChunkKey, GLOWSTONE, STONE};

const SIDE: usize = CHUNK_SIZE * 3;
const PLANE: usize = SIDE * SIDE;
const VOLUME: usize = SIDE * SIDE * SIDE;
const MAX_LIGHT: u8 = 15;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightSample {
    pub sky: u8,
    pub glow: u8,
}

pub struct LightField {
    sky: Vec<u8>,
    glow: Vec<u8>,
}

impl LightField {
    /// The one-chunk halo is wider than the 15-step propagation range. Chunks
    /// not yet streamed use the deterministic baseline until their snapshot
    /// arrives, at which point the client re-lights affected neighbors.
    pub fn build(key: ChunkKey, known: &HashMap<ChunkKey, Arc<Chunk>>, seed: u64) -> Self {
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
                            blocks[target..target + CHUNK_SIZE]
                                .copy_from_slice(&chunk.blocks[source..source + CHUNK_SIZE]);
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
                    if blocks[at] == GLOWSTONE {
                        glow[at] = MAX_LIGHT;
                        glow_frontier.push_back(at);
                    }
                    if blocks[at] != AIR {
                        open_to_sky = false;
                    } else if open_to_sky {
                        sky[at] = MAX_LIGHT;
                        sky_frontier.push_back(at);
                    }
                }
            }
        }
        propagate(&blocks, &mut sky, sky_frontier);
        propagate(&blocks, &mut glow, glow_frontier);
        Self { sky, glow }
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
    ) -> [f32; 2] {
        let [axis, u, v] = axes;
        let [corner_u, corner_v] = corner;
        let mut sky = 0u32;
        let mut glow = 0u32;
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
            }
        }
        [sky as f32 / 60.0, glow as f32 / 60.0]
    }
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

fn propagate(blocks: &[u8], light: &mut [u8], mut frontier: VecDeque<usize>) {
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
            if blocks[neighbor] == AIR && light[neighbor] < next {
                light[neighbor] = next;
                frontier.push_back(neighbor);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sealed_neighborhood(key: ChunkKey) -> HashMap<ChunkKey, Arc<Chunk>> {
        let mut known = HashMap::new();
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let neighbor = key_offset(key, dx, dy, dz).unwrap();
                    known.insert(
                        neighbor,
                        Arc::new(Chunk {
                            key: neighbor,
                            version: 0,
                            blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
                        }),
                    );
                }
            }
        }
        known
    }

    #[test]
    fn sealed_cave_is_dark_and_a_lamp_propagates() {
        let key = ChunkKey { x: 0, y: 1, z: 0 };
        let mut known = sealed_neighborhood(key);
        let room = Arc::make_mut(known.get_mut(&key).unwrap());
        for y in 5..=7 {
            for z in 5..=9 {
                for x in 5..=9 {
                    room.blocks[Chunk::index([x, y, z]).unwrap()] = AIR;
                }
            }
        }
        let dark = LightField::build(key, &known, 0xB10C_6100);
        assert_eq!(dark.face([7, 4, 7], 1, 1), LightSample::default());

        Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([7, 6, 7]).unwrap()] =
            GLOWSTONE;
        let lit = LightField::build(key, &known, 0xB10C_6100);
        assert_eq!(lit.face([7, 4, 7], 1, 1).glow, 14);
        assert_eq!(lit.face([7, 4, 7], 1, 1).sky, 0);
    }

    #[test]
    fn opening_a_roof_shaft_relights_the_cave() {
        let key = ChunkKey { x: 0, y: 1, z: 0 };
        let mut known = sealed_neighborhood(key);
        for y in 5..=15 {
            Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([8, y, 8]).unwrap()] =
                AIR;
        }
        let above = key_offset(key, 0, 1, 0).unwrap();
        for y in 0..CHUNK_SIZE {
            Arc::make_mut(known.get_mut(&above).unwrap()).blocks
                [Chunk::index([8, y, 8]).unwrap()] = AIR;
        }
        let field = LightField::build(key, &known, 0xB10C_6100);
        assert_eq!(field.face([8, 5, 8], 1, 1).sky, 15);
        Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([8, 8, 8]).unwrap()] =
            STONE;
        let closed = LightField::build(key, &known, 0xB10C_6100);
        assert_eq!(closed.face([8, 5, 8], 1, 1).sky, 0);
    }

    #[test]
    fn emitted_light_crosses_chunk_seams_and_removal_darkens_both_sides() {
        let key = ChunkKey { x: 0, y: 1, z: 0 };
        let west = key_offset(key, -1, 0, 0).unwrap();
        let mut known = sealed_neighborhood(key);
        Arc::make_mut(known.get_mut(&west).unwrap()).blocks[Chunk::index([15, 6, 8]).unwrap()] =
            GLOWSTONE;
        for x in 0..=3 {
            Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([x, 6, 8]).unwrap()] =
                AIR;
        }
        let lit = LightField::build(key, &known, 0xB10C_6100);
        assert_eq!(lit.face([2, 5, 8], 1, 1).glow, 12);
        Arc::make_mut(known.get_mut(&west).unwrap()).blocks[Chunk::index([15, 6, 8]).unwrap()] =
            STONE;
        let dark = LightField::build(key, &known, 0xB10C_6100);
        assert_eq!(dark.face([2, 5, 8], 1, 1).glow, 0);
    }
}
