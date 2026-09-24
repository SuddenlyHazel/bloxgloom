//! Authoritative drop motion and edit-triggered wakeups.
use crate::world::{ChunkKey, World, world_to_chunk};
use std::time::Duration;

use super::{DROP_RADIUS, Drops, GRAVITY, TERMINAL_SPEED};

#[cfg(test)]
#[path = "physics/tests.rs"]
mod tests;

const MAX_MISSING_CHUNKS_PER_STEP: usize = 64;

#[derive(Default)]
pub(in crate::server) struct DropStepResult {
    pub moved: bool,
    pub landed: bool,
    pub missing_chunks: Vec<ChunkKey>,
}

impl Drops {
    /// Advances only airborne drops. Positions, not visual animation, are authoritative.
    pub(in crate::server) fn step(&mut self, world: &World, elapsed: Duration) -> DropStepResult {
        if self.active.is_empty() {
            return DropStepResult::default();
        }
        let dt = elapsed.as_secs_f32().min(0.1);
        if dt <= 0.0 {
            return DropStepResult::default();
        }
        // The ordered active index gives deterministic work order without
        // copying and sorting every airborne ID on every tick. Missing
        // terrain defers that drop, so it cannot fall through unknown voxels.
        let mut settled_ids = Vec::new();
        let mut missing_chunks = Vec::new();
        let mut moved = false;
        let mut landed = false;
        for &id in &self.active {
            let entry = self.entries.get_mut(&id).expect("active drop exists");
            let start = entry.position[1] - DROP_RADIUS;
            let speed = (entry.vertical_speed - GRAVITY * dt).max(-TERMINAL_SPEED);
            let end = start + speed * dt;
            let (new_y, new_speed, settled) =
                match first_solid_top(world, entry.position, start, end, &mut missing_chunks) {
                    TerrainCheck::Missing => continue,
                    TerrainCheck::Hit(top) => (top + DROP_RADIUS, 0.0, true),
                    TerrainCheck::Clear => (end + DROP_RADIUS, speed, false),
                };
            // The BGDP snapshot stores exact f32 bits. Even a sub-pixel move
            // must advance its revision or a checkpoint receipt could mistake
            // an older snapshot for the current authoritative position.
            moved |= entry.position[1].to_bits() != new_y.to_bits();
            landed |= settled;
            entry.position[1] = new_y;
            entry.vertical_speed = new_speed;
            self.spatial.move_to(id, entry.position);
            if settled {
                settled_ids.push(id);
            }
        }
        for id in settled_ids {
            self.active.remove(&id);
        }
        if moved {
            self.revision = self.revision.wrapping_add(1);
        }
        missing_chunks.sort_by_key(|key| (key.x, key.y, key.z));
        DropStepResult {
            moved,
            landed,
            missing_chunks,
        }
    }

    /// An edited voxel can remove support or intersect an otherwise sleeping drop.
    pub(in crate::server) fn wake_near(&mut self, block: [i32; 3]) {
        let block_x = block[0] as f32;
        let block_y = block[1] as f32;
        let block_z = block[2] as f32;
        let min = [
            block_x - DROP_RADIUS,
            block_y + 1.0 + DROP_RADIUS - 1.1,
            block_z - DROP_RADIUS,
        ];
        let max = [
            block_x + 1.0 + DROP_RADIUS,
            block_y + 1.0 + DROP_RADIUS + 1.1,
            block_z + 1.0 + DROP_RADIUS,
        ];
        for id in self.spatial.query_aabb(min, max) {
            let Some(entry) = self.entries.get(&id) else {
                continue;
            };
            let [x, y, z] = entry.position;
            if x + DROP_RADIUS > block_x
                && x - DROP_RADIUS < block_x + 1.0
                && z + DROP_RADIUS > block_z
                && z - DROP_RADIUS < block_z + 1.0
                && (y - (block_y + 1.0 + DROP_RADIUS)).abs() < 1.1
            {
                self.active.insert(id);
            }
        }
    }
}

enum TerrainCheck {
    Hit(f32),
    Clear,
    Missing,
}

fn first_solid_top(
    world: &World,
    position: [f32; 3],
    start_bottom: f32,
    end_bottom: f32,
    missing_chunks: &mut Vec<ChunkKey>,
) -> TerrainCheck {
    let mut hit: Option<f32> = None;
    let bottom = end_bottom.floor() as i32;
    let top = start_bottom.floor() as i32;
    for y in (bottom..=top).rev() {
        let block_top = y as f32 + 1.0;
        if block_top < end_bottom {
            continue;
        }
        for x in [position[0] - DROP_RADIUS, position[0] + DROP_RADIUS] {
            for z in [position[2] - DROP_RADIUS, position[2] + DROP_RADIUS] {
                let x = x.floor() as i32;
                let z = z.floor() as i32;
                match world.cached_block(x, y, z) {
                    Some(block)
                        if world.catalog().block_flags(block) & crate::content::SOLID != 0 =>
                    {
                        hit = Some(hit.map_or(block_top, |previous| previous.max(block_top)));
                    }
                    Some(_) => {}
                    None => {
                        let key = world_to_chunk(x, y, z).0;
                        if missing_chunks.len() < MAX_MISSING_CHUNKS_PER_STEP
                            && !missing_chunks.contains(&key)
                        {
                            missing_chunks.push(key);
                        }
                        return TerrainCheck::Missing;
                    }
                }
            }
        }
    }
    hit.map_or(TerrainCheck::Clear, TerrainCheck::Hit)
}
