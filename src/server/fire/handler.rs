//! Pure owner-local burn candidate computation on immutable chunk snapshots.

use super::{FireFrontier, FireIgnition, FireIgnitionId, FirePending};
use crate::content::{Catalog, FLAMMABLE};
use crate::server::effects::CellCoord;
use crate::server::parallel::{OwnerJob, OwnerKey, OwnerPatch, PatchUsage};
use crate::server::registry::{SystemHandler, SystemHandlerError};
use crate::world::{AIR, Chunk, ChunkKey, EditBasis, PreparedEdit};
use std::collections::BTreeMap;
use std::sync::Arc;

pub(super) const MAX_DUE_CELLS_PER_OWNER: usize = 32;
pub(super) const MAX_IGNITIONS_PER_OWNER: usize = MAX_DUE_CELLS_PER_OWNER * 6;
pub(super) const MAX_DELIVERIES_PER_OWNER: usize = 192;

#[derive(Clone)]
pub(super) struct FireOwnerInput {
    pub(super) basis: Arc<EditBasis>,
    pub(super) frontier: Arc<FireFrontier>,
    pub(super) mailboxes: BTreeMap<ChunkKey, Arc<FirePending>>,
    pub(super) tick: u64,
    pub(super) max_due: usize,
}

/// The only output a fire handler may produce. The runtime validates all
/// destinations and turns it into exact WAL before/after values.
#[derive(Clone, Debug)]
pub(super) struct FireOwnerPatch {
    pub(super) owner: ChunkKey,
    pub(super) expected_chunk_version: u64,
    pub(super) consumed: Vec<(u16, u64)>,
    pub(super) frontier_after: FireFrontier,
    pub(super) burns: Vec<u16>,
    pub(super) changed_cells: Vec<CellCoord>,
    pub(super) mailbox_after: BTreeMap<ChunkKey, FirePending>,
    pub(super) effect_count: usize,
    pub(super) world_edit: Option<PreparedEdit>,
}

#[derive(Clone)]
pub(super) struct FireDeliveryInput {
    pub(super) chunk: Arc<Chunk>,
    pub(super) frontier: Arc<FireFrontier>,
    pub(super) source: ChunkKey,
    pub(super) mailbox: Arc<FirePending>,
    pub(super) catalog: Arc<Catalog>,
}

#[derive(Clone, Debug)]
pub(super) struct FireDeliveryPatch {
    pub(super) owner: ChunkKey,
    pub(super) source: ChunkKey,
    pub(super) expected_chunk_version: u64,
    pub(super) frontier_after: FireFrontier,
    pub(super) pending_after: FirePending,
    pub(super) consumed: usize,
}

#[derive(Clone, Copy, Default)]
pub(in crate::server) struct FireHandler;

#[derive(Clone, Copy, Default)]
pub(in crate::server) struct FireDeliveryHandler;

impl SystemHandler for FireDeliveryHandler {
    fn prepare(&self, job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
        let OwnerKey::Chunk(owner) = job.owner() else {
            return Err(rejected("fire delivery owner is not a chunk"));
        };
        let input = job
            .snapshot(job.owner())
            .and_then(|snapshot| snapshot.value::<FireDeliveryInput>())
            .ok_or_else(|| rejected("missing fire delivery snapshot"))?;
        if input.chunk.key != owner || input.mailbox.is_empty() {
            return Err(rejected("invalid fire delivery inputs"));
        }
        let mut frontier_after = (*input.frontier).clone();
        let mut pending_after = (*input.mailbox).clone();
        let mut consumed = 0;
        for &ignition in input.mailbox.iter().take(MAX_DELIVERIES_PER_OWNER) {
            let target = input
                .chunk
                .block_index(usize::from(ignition.target_cell))
                .ok_or_else(|| rejected("fire ignition target outside chunk"))?;
            if input.catalog.block_flags(target) & FLAMMABLE != 0
                && frontier_after
                    .insert(ignition.target_cell, ignition.activate_at)
                    .is_err()
            {
                // Destination frontier is full. Keep this ignition in its
                // durable mailbox; a later source burn can make room.
                continue;
            }
            pending_after.remove(ignition.id);
            consumed += 1;
        }
        Ok(OwnerPatch::new(
            job,
            FireDeliveryPatch {
                owner,
                source: input.source,
                expected_chunk_version: input.chunk.version,
                frontier_after,
                pending_after,
                consumed,
            },
            PatchUsage {
                writes: consumed,
                effects: 0,
                estimated_bytes: 64 + consumed * 31,
            },
        ))
    }
}

impl SystemHandler for FireHandler {
    fn prepare(&self, job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
        let OwnerKey::Chunk(owner) = job.owner() else {
            return Err(rejected("fire job owner is not a chunk"));
        };
        let input = job
            .snapshot(job.owner())
            .and_then(|snapshot| snapshot.value::<FireOwnerInput>())
            .ok_or_else(|| rejected("missing fire owner snapshot"))?;
        if input.basis.chunk().key != owner || input.max_due > MAX_DUE_CELLS_PER_OWNER {
            return Err(rejected("invalid fire job inputs"));
        }
        let activation = input
            .tick
            .checked_add(1)
            .ok_or_else(|| rejected("fire simulation tick exhausted"))?;
        let consumed = input.frontier.due(input.tick, input.max_due);
        let mut frontier_after = (*input.frontier).clone();
        let mut burns = Vec::new();
        let mut ignitions = Vec::new();
        for &(cell, scheduled) in &consumed {
            let removed = frontier_after.remove(cell);
            if removed != Some(scheduled) {
                return Err(rejected("fire frontier changed during preparation"));
            }
            let block = input
                .basis
                .chunk()
                .block_index(usize::from(cell))
                .ok_or_else(|| rejected("fire cell out of chunk bounds"))?;
            if input.basis.catalog().block_flags(block) & FLAMMABLE == 0 {
                continue;
            }
            burns.push(cell);
            for direction in 0..6u8 {
                let (destination, target_cell) = neighbor(owner, cell, direction)
                    .ok_or_else(|| rejected("fire crossed world coordinate limit"))?;
                ignitions.push((
                    destination,
                    FireIgnition {
                        id: FireIgnitionId {
                            source_tick: scheduled,
                            source_cell: cell,
                            direction,
                        },
                        target_cell,
                        activate_at: activation,
                    },
                ));
            }
        }
        // Sorting does not depend on worker completion or hash-table order.
        ignitions.sort_unstable_by_key(|&(destination, ignition)| (destination, ignition.id));
        if ignitions.len() > MAX_IGNITIONS_PER_OWNER {
            return Err(rejected("fire handler exceeded ignition budget"));
        }
        let mut mailbox_after = BTreeMap::<ChunkKey, FirePending>::new();
        for &(destination, ignition) in &ignitions {
            let mailbox = mailbox_after.entry(destination).or_insert_with(|| {
                input
                    .mailboxes
                    .get(&destination)
                    .map_or_else(FirePending::default, |mailbox| (**mailbox).clone())
            });
            if let Err(error) = mailbox.insert(ignition) {
                if error.kind() != std::io::ErrorKind::Other {
                    return Err(SystemHandlerError::Rejected(format!(
                        "fire mailbox merge: {error}"
                    )));
                }
                // A full mailbox defers the entire source owner. The original
                // frontier remains intact until a later successful WAL plan.
                return Ok(OwnerPatch::new(
                    job,
                    FireOwnerPatch {
                        owner,
                        expected_chunk_version: input.basis.chunk().version,
                        consumed: Vec::new(),
                        frontier_after: (*input.frontier).clone(),
                        burns: Vec::new(),
                        changed_cells: Vec::new(),
                        mailbox_after: BTreeMap::new(),
                        effect_count: 0,
                        world_edit: None,
                    },
                    PatchUsage::default(),
                ));
            }
        }
        let world_edit = if burns.is_empty() {
            None
        } else {
            let edits: Vec<_> = burns.iter().map(|&cell| (cell, AIR)).collect();
            let prepared = input.basis.prepare_sparse(&edits).map_err(|error| {
                SystemHandlerError::Rejected(format!("fire edit prepare: {error}"))
            })?;
            if !prepared.changed {
                return Err(rejected("fire burn produced unchanged world edit"));
            }
            Some(prepared)
        };
        let changed_cells = burns
            .iter()
            .filter_map(|&cell| world_cell(owner, cell))
            .collect();
        let usage = PatchUsage {
            writes: consumed.len() + burns.len(),
            effects: ignitions.len(),
            estimated_bytes: 64
                + consumed.len() * 10
                + burns.len() * 2
                + ignitions.len() * 33
                + mailbox_after
                    .values()
                    .map(|mailbox| mailbox.len() * 21)
                    .sum::<usize>()
                + world_edit
                    .as_ref()
                    .map_or(0, |edit| edit.after_snapshot.len()),
        };
        Ok(OwnerPatch::new(
            job,
            FireOwnerPatch {
                owner,
                expected_chunk_version: input.basis.chunk().version,
                consumed,
                frontier_after,
                burns,
                changed_cells,
                mailbox_after,
                effect_count: ignitions.len(),
                world_edit,
            },
            usage,
        ))
    }
}

pub(super) fn neighbor(source: ChunkKey, cell: u16, direction: u8) -> Option<(ChunkKey, u16)> {
    let mut local = [
        i32::from(cell % 16),
        i32::from(cell / 256),
        i32::from((cell / 16) % 16),
    ];
    let axis = usize::from(direction / 2);
    local[axis] += if direction & 1 == 0 { -1 } else { 1 };
    let mut chunk = [source.x, source.y, source.z];
    if local[axis] < 0 {
        chunk[axis] = chunk[axis].checked_sub(1)?;
        local[axis] = 15;
    } else if local[axis] == 16 {
        chunk[axis] = chunk[axis].checked_add(1)?;
        local[axis] = 0;
    }
    let target = local[0] + 16 * (local[2] + 16 * local[1]);
    Some((
        ChunkKey {
            x: chunk[0],
            y: chunk[1],
            z: chunk[2],
        },
        target as u16,
    ))
}

fn rejected(reason: &'static str) -> SystemHandlerError {
    SystemHandlerError::Rejected(reason.into())
}

fn world_cell(owner: ChunkKey, cell: u16) -> Option<CellCoord> {
    let local = [
        i64::from(cell % 16),
        i64::from(cell / 256),
        i64::from((cell / 16) % 16),
    ];
    let coordinates = [owner.x, owner.y, owner.z].map(i64::from);
    Some(CellCoord::new(
        i32::try_from(coordinates[0] * 16 + local[0]).ok()?,
        i32::try_from(coordinates[1] * 16 + local[1]).ok()?,
        i32::try_from(coordinates[2] * 16 + local[2]).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbor_crosses_each_seam_and_preserves_local_order() {
        let source = ChunkKey { x: 3, y: -2, z: 8 };
        let low = 0u16;
        assert_eq!(
            neighbor(source, low, 0),
            Some((ChunkKey { x: 2, ..source }, 15))
        );
        assert_eq!(
            neighbor(source, low, 2),
            Some((ChunkKey { y: -3, ..source }, 3_840))
        );
        assert_eq!(
            neighbor(source, low, 4),
            Some((ChunkKey { z: 7, ..source }, 240))
        );
        assert_eq!(neighbor(source, low, 1), Some((source, 1)));
    }
}
