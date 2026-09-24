//! Deduplicated bounded per-chunk scheduled cells.

use super::codec::{checked_body, finish, invalid};
use std::collections::{BTreeMap, BTreeSet};
use std::io;

pub(super) const MAX_FRONTIER_CELLS: usize = 4_096;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct FireFrontier {
    cells: BTreeMap<u16, u64>,
    by_tick: BTreeSet<(u64, u16)>,
}

impl FireFrontier {
    pub(super) fn len(&self) -> usize {
        self.cells.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Repeated ignitions keep the earliest original scheduled tick.
    pub(super) fn insert(&mut self, cell: u16, scheduled_tick: u64) -> io::Result<bool> {
        if usize::from(cell) >= MAX_FRONTIER_CELLS || scheduled_tick == 0 {
            return Err(invalid("invalid scheduled fire cell"));
        }
        if let Some(existing) = self.cells.get_mut(&cell) {
            if scheduled_tick < *existing {
                self.by_tick.remove(&(*existing, cell));
                *existing = scheduled_tick;
                self.by_tick.insert((scheduled_tick, cell));
                return Ok(true);
            }
            return Ok(false);
        }
        if self.cells.len() == MAX_FRONTIER_CELLS {
            return Err(io::Error::other("fire frontier full"));
        }
        self.cells.insert(cell, scheduled_tick);
        self.by_tick.insert((scheduled_tick, cell));
        Ok(true)
    }

    pub(super) fn remove(&mut self, cell: u16) -> Option<u64> {
        let tick = self.cells.remove(&cell)?;
        self.by_tick.remove(&(tick, cell));
        Some(tick)
    }

    pub(super) fn due(&self, tick: u64, limit: usize) -> Vec<(u16, u64)> {
        self.by_tick
            .iter()
            .take_while(|&&(scheduled, _)| scheduled <= tick)
            .take(limit)
            .map(|&(scheduled, cell)| (cell, scheduled))
            .collect()
    }

    pub(super) fn is_due(&self, tick: u64) -> bool {
        self.by_tick
            .first()
            .is_some_and(|&(scheduled, _)| scheduled <= tick)
    }

    pub(super) fn latest_tick(&self) -> u64 {
        self.by_tick.last().map_or(0, |&(scheduled, _)| scheduled)
    }

    pub(super) fn encode(&self) -> Vec<u8> {
        if self.cells.is_empty() {
            return Vec::new();
        }
        let mut bytes = Vec::with_capacity(11 + self.cells.len() * 10);
        bytes.extend(b"BGFF");
        bytes.push(1);
        bytes.extend((self.cells.len() as u16).to_le_bytes());
        for (&cell, &tick) in &self.cells {
            bytes.extend(cell.to_le_bytes());
            bytes.extend(tick.to_le_bytes());
        }
        finish(bytes)
    }

    pub(super) fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        let body = checked_body(bytes, b"BGFF", 10)?;
        if body.len() / 10 > MAX_FRONTIER_CELLS {
            return Err(invalid("fire frontier exceeds bound"));
        }
        let mut cells = BTreeMap::new();
        let mut by_tick = BTreeSet::new();
        for record in body.chunks_exact(10) {
            let cell = u16::from_le_bytes(record[..2].try_into().unwrap());
            let scheduled = u64::from_le_bytes(record[2..].try_into().unwrap());
            if usize::from(cell) >= MAX_FRONTIER_CELLS
                || scheduled == 0
                || cells.insert(cell, scheduled).is_some()
            {
                return Err(invalid("invalid or duplicate fire frontier cell"));
            }
            by_tick.insert((scheduled, cell));
        }
        Ok(Self { cells, by_tick })
    }
}
