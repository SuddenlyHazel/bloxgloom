//! Bounded rotating scans and delayed cell attempts. Timers are latency hints;
//! eligibility is rechecked by the durable planner, never trusted from here.
use super::rules::{Cell, LOG_RANGE, Rule};
use crate::world::{CHUNK_SIZE, CHUNK_VOLUME, ChunkKey};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::ops::Bound;

pub(super) const MAX_PENDING: usize = 4_096;
const MAX_HINTS: usize = 8_192;
const CHUNKS_PER_TICK: usize = 8;
const CELLS_PER_CHUNK: usize = 32;
const HINTS_PER_TICK: usize = 64;
pub(super) const MAX_QUEUED: usize = 16;
pub(super) const DUE_PER_TICK: usize = 4;

#[derive(Default)]
pub(in crate::server) struct Runtime {
    chunks: BTreeMap<ChunkKey, usize>,
    next_chunk: Option<ChunkKey>,
    pending: BTreeMap<Cell, (Rule, u64)>,
    due: BTreeSet<(u64, Cell)>,
    hints: VecDeque<Cell>,
    hinted: HashSet<Cell>,
    queued: HashSet<Cell>,
}

impl Runtime {
    pub(super) fn refresh(&mut self, keys: BTreeSet<ChunkKey>) {
        self.chunks.retain(|key, _| keys.contains(key));
        for key in keys {
            self.chunks.entry(key).or_insert(0);
        }
    }

    fn hint(&mut self, cell: Cell) {
        if self.hints.len() < MAX_HINTS && self.hinted.insert(cell) {
            self.hints.push_back(cell);
        }
    }

    pub(in crate::server) fn changed(&mut self, cell: Cell, block: crate::world::BlockId) {
        self.hint(cell);
        if let Some(y) = cell[1].checked_sub(1) {
            self.hint([cell[0], y, cell[2]]);
        }
        if matches!(
            block,
            crate::world::AIR | crate::world::WOOD | crate::world::WOOD_X | crate::world::WOOD_Z
        ) {
            let range = i32::from(LOG_RANGE);
            for dz in -range..=range {
                for dy in -range..=range {
                    for dx in -range..=range {
                        if let (Some(x), Some(y), Some(z)) = (
                            cell[0].checked_add(dx),
                            cell[1].checked_add(dy),
                            cell[2].checked_add(dz),
                        ) {
                            self.hint([x, y, z]);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn samples(&mut self) -> Vec<Cell> {
        let mut result = Vec::with_capacity(HINTS_PER_TICK + CHUNKS_PER_TICK * CELLS_PER_CHUNK);
        for _ in 0..HINTS_PER_TICK {
            let Some(cell) = self.hints.pop_front() else {
                break;
            };
            self.hinted.remove(&cell);
            result.push(cell);
        }
        let len = self.chunks.len();
        if len == 0 {
            return result;
        }
        let keys: Vec<_> = self
            .chunks
            .range((
                self.next_chunk.map_or(Bound::Unbounded, Bound::Excluded),
                Bound::Unbounded,
            ))
            .chain(self.chunks.iter())
            .take(CHUNKS_PER_TICK.min(len))
            .map(|(&key, _)| key)
            .collect();
        self.next_chunk = keys.last().copied();
        for key in keys {
            let cursor = self.chunks.get_mut(&key).unwrap();
            for _ in 0..CELLS_PER_CHUNK {
                // Odd stride visits every voxel, spreading checks over the chunk.
                let index = (*cursor * 1_549) % CHUNK_VOLUME;
                *cursor = (*cursor + 1) % CHUNK_VOLUME;
                let local = [
                    index % CHUNK_SIZE,
                    index / (CHUNK_SIZE * CHUNK_SIZE),
                    (index / CHUNK_SIZE) % CHUNK_SIZE,
                ];
                let coordinate = [key.x, key.y, key.z].map(|v| i64::from(v) * CHUNK_SIZE as i64);
                if let (Ok(x), Ok(y), Ok(z)) = (
                    i32::try_from(coordinate[0] + local[0] as i64),
                    i32::try_from(coordinate[1] + local[1] as i64),
                    i32::try_from(coordinate[2] + local[2] as i64),
                ) {
                    result.push([x, y, z]);
                }
            }
        }
        result
    }

    pub(super) fn observe(&mut self, cell: Cell, rule: Option<Rule>, tick: u64, seed: u64) {
        if self.queued.contains(&cell) {
            return;
        }
        if self.pending.get(&cell).map(|(old, _)| *old) == rule {
            return;
        }
        if let Some((_, due)) = self.pending.remove(&cell) {
            self.due.remove(&(due, cell));
        }
        if let Some(rule) = rule
            && self.pending.len() < MAX_PENDING
        {
            let due = tick.saturating_add(rule.delay(seed, cell));
            self.pending.insert(cell, (rule, due));
            self.due.insert((due, cell));
        }
    }

    pub(super) fn take_due(&mut self, tick: u64) -> Option<(Cell, Rule)> {
        if self.queued.len() >= MAX_QUEUED {
            return None;
        }
        let &(due, cell) = self.due.first()?;
        if due > tick {
            return None;
        }
        self.due.pop_first();
        let (rule, _) = self.pending.remove(&cell).unwrap();
        self.queued.insert(cell);
        Some((cell, rule))
    }

    pub(in crate::server) fn finished(&mut self, cell: Cell) {
        self.queued.remove(&cell);
    }
}
