//! Pure eligibility checks over resident authoritative terrain.
use crate::server::durable::TerrainReads;
use crate::{
    content::{OPAQUE, SOLID},
    world::{self, BlockId, ChunkKey, World},
};
use std::{
    collections::{HashSet, VecDeque},
    io,
};

pub(super) const LOG_RANGE: u8 = 6;
const MAX_SKY_SCAN: i32 = 512;
pub(in crate::server) type Cell = [i32; 3];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::server) enum Rule {
    LeafDecay,
    GrassDeath,
    GrassGrowth,
}

impl Rule {
    pub(super) fn delay(self, seed: u64, cell: Cell) -> u64 {
        let random = bloxgloom_host_api::gameplay::cell_random(seed, cell, 0);
        let (minimum, span) = match self {
            Self::LeafDecay => (250, 500),
            Self::GrassDeath => (500, 1_000),
            Self::GrassGrowth => (1_000, 2_000),
        };
        minimum + random % span
    }

    pub(in crate::server) fn transition(self) -> (BlockId, BlockId) {
        match self {
            Self::LeafDecay => (world::LEAVES, world::AIR),
            Self::GrassDeath => (world::GRASS, world::DIRT),
            Self::GrassGrowth => (world::DIRT, world::GRASS),
        }
    }
}

fn read(
    world: &mut World,
    reads: &mut TerrainReads,
    missing: &mut Vec<ChunkKey>,
    cell: Cell,
) -> io::Result<BlockId> {
    if let Some(block) = reads.read(world, cell[0], cell[1], cell[2])? {
        return Ok(block);
    }
    let key = world::world_to_chunk(cell[0], cell[1], cell[2]).0;
    if !missing.contains(&key) {
        missing.push(key);
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "ecology terrain unavailable",
    ))
}

pub(in crate::server) fn daylight(elapsed_ms: u64) -> bool {
    (0.02..0.48).contains(&crate::daylight::phase(elapsed_ms))
}

pub(in crate::server) fn check(
    world: &mut World,
    reads: &mut TerrainReads,
    missing: &mut Vec<ChunkKey>,
    cell: Cell,
    sun_up: bool,
) -> io::Result<Option<Rule>> {
    let block = read(world, reads, missing, cell)?;
    if crate::content::jg_rtx::is_leaf(world.catalog(), block) {
        return supported(world, reads, missing, cell)
            .map(|supported| (!supported).then_some(Rule::LeafDecay));
    }
    if block != world::GRASS && block != world::DIRT {
        return Ok(None);
    }
    let Some(y) = cell[1].checked_add(1) else {
        return Ok(None);
    };
    let above = read(world, reads, missing, [cell[0], y, cell[2]])?;
    let covered = world.catalog().block_flags(above) & SOLID != 0;
    if block == world::GRASS {
        return Ok(covered.then_some(Rule::GrassDeath));
    }
    if covered || !sun_up {
        return Ok(None);
    }
    let Some(top) = world.sky_scan_top(cell[0], cell[2]) else {
        return Ok(None);
    };
    let top = top.max(y);
    if i64::from(top) - i64::from(y) >= i64::from(MAX_SKY_SCAN) {
        return Ok(None);
    }
    for sy in y..=top {
        let sample = read(world, reads, missing, [cell[0], sy, cell[2]])?;
        if world.catalog().block_flags(sample) & OPAQUE != 0 {
            return Ok(None);
        }
    }
    Ok(Some(Rule::GrassGrowth))
}

fn supported(
    world: &mut World,
    reads: &mut TerrainReads,
    missing: &mut Vec<ChunkKey>,
    cell: Cell,
) -> io::Result<bool> {
    let mut queue = VecDeque::from([(cell, 0u8)]);
    let mut visited = HashSet::from([cell]);
    while let Some((at, distance)) = queue.pop_front() {
        for (axis, delta) in [(0, -1), (0, 1), (1, -1), (1, 1), (2, -1), (2, 1)] {
            let mut next = at;
            let Some(value) = next[axis].checked_add(delta) else {
                continue;
            };
            next[axis] = value;
            if !visited.insert(next) {
                continue;
            }
            let block = read(world, reads, missing, next)?;
            if crate::content::jg_rtx::is_log(world.catalog(), block) {
                return Ok(true);
            }
            if crate::content::jg_rtx::is_leaf(world.catalog(), block) && distance + 1 < LOG_RANGE {
                queue.push_back((next, distance + 1));
            }
        }
    }
    Ok(false)
}
