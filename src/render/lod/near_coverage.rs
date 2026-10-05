//! Upload known-ready 3D coverage only when membership changes; reuse CPU storage.
use crate::world::ChunkKey;
use std::collections::HashSet;
pub(super) const SLOTS: usize = 16384;
#[derive(Default)]
pub(super) struct NearCoverage {
    ready: HashSet<ChunkKey>,
    scratch: HashSet<ChunkKey>,
    slots: Vec<[i32; 4]>,
}
impl NearCoverage {
    pub(super) fn update(&mut self, near: impl Iterator<Item = ChunkKey>) -> Option<&[[i32; 4]]> {
        self.scratch.clear();
        self.scratch.extend(near);
        if self.ready == self.scratch && !self.slots.is_empty() {
            return None;
        }
        std::mem::swap(&mut self.ready, &mut self.scratch);
        self.slots.resize(SLOTS, [0; 4]);
        self.slots.fill([0; 4]);
        for k in &self.ready {
            let mut index = hash(*k);
            for _ in 0..SLOTS {
                if self.slots[index][3] == 0 {
                    self.slots[index] = [k.x, k.y, k.z, 1];
                    break;
                }
                index = (index + 1) & (SLOTS - 1);
            }
        }
        Some(&self.slots)
    }
}
fn hash(k: ChunkKey) -> usize {
    ((k.x as u32).wrapping_mul(73856093)
        ^ (k.y as u32).wrapping_mul(19349663)
        ^ (k.z as u32).wrapping_mul(83492791)) as usize
        & (SLOTS - 1)
}
#[cfg(test)]
mod tests;
