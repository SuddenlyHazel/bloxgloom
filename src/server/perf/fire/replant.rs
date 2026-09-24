//! Bounded benchmark-directed wood replenishment through ordinary action WAL.
//!
//! The finite initial forest cannot sustain a long fire soak. Each successful
//! transaction replants only cells observed as committed AIR in one resident
//! owner; admission failure leaves them untouched for a later attempt.

use super::*;
use crate::world::AIR;

const MAX_REPLANT_CELLS: usize = 1_024;
const OWNER_PROBES_PER_TICK: usize = 4;

#[derive(Default)]
pub(super) struct Replanter {
    next_owner: usize,
    pub(super) admitted: u64,
    pub(super) deferred: u64,
    pub(super) replanted: u64,
}

impl Replanter {
    pub(super) fn attempt(
        &mut self,
        state: &mut State,
        owners: &[ChunkKey],
        tick: TickId,
    ) -> io::Result<()> {
        for _ in 0..OWNER_PROBES_PER_TICK {
            let owner = owners[self.next_owner % owners.len()];
            self.next_owner += 1;
            if state
                .durability
                .reserved
                .contains(&durable::chunk_state_key(owner))
            {
                continue;
            }
            let chunk = state
                .world
                .cached_arc_chunk(owner)
                .ok_or_else(|| io::Error::other("fire replant owner is not resident"))?;
            let mut edits = Vec::with_capacity(MAX_REPLANT_CELLS);
            for index in 0..4_096 {
                if chunk.block_index(index) != Some(AIR) {
                    continue;
                }
                let local_x = (index % 16) as i32;
                let local_z = ((index / 16) % 16) as i32;
                let local_y = (index / 256) as i32;
                edits.push((
                    owner.x * 16 + local_x,
                    owner.y * 16 + local_y,
                    owner.z * 16 + local_z,
                    WOOD,
                ));
                if edits.len() == MAX_REPLANT_CELLS {
                    break;
                }
            }
            if edits.is_empty() {
                continue;
            }
            let replanted = edits.len() as u64;
            let prepared = state.world.prepare_edits(&edits)?;
            let action = fixture_action(prepared, None);
            match state.durability.try_stage(tick, &action, None, None) {
                Ok(true) => {
                    self.admitted += 1;
                    self.replanted += replanted;
                    return Ok(());
                }
                Ok(false) => return Err(io::Error::other("replant WAL action was empty")),
                Err(StageError::Conflict | StageError::Full) => {
                    self.deferred += 1;
                }
                Err(error) => return Err(stage_error(error)),
            }
        }
        Ok(())
    }
}
