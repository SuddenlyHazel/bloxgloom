//! Checkpoint-gated journal generation rotation.

use super::*;
use crate::server::State;

/// Admissions stop at the WAL soft cap; already accepted transactions drain
/// and become visible before the final BGDP snapshot and all dirty BGED/BGIN/
/// BGDP files are fenced.
pub(super) fn progress_rotation(state: &mut State) -> io::Result<bool> {
    if let Some(receiver) = &state.durability.rotation_receipt {
        match receiver.try_recv() {
            Ok(Ok(receipt)) => {
                if receipt.cut_sequence != state.durability.writer.sequence() {
                    state.durability.failed = true;
                    return Err(io::Error::other(
                        "journal rotation returned an unexpected sequence",
                    ));
                }
                state.durability.rotation_receipt = None;
                state.durability.rotation_requested = false;
                state.durability.rotation_snapshot_ready = false;
                return Ok(false);
            }
            Ok(Err(error)) => {
                state.durability.failed = true;
                return Err(io::Error::other(format!(
                    "journal rotation failed: {error}"
                )));
            }
            Err(TryRecvError::Empty) => return Ok(true),
            Err(TryRecvError::Disconnected) => {
                state.durability.failed = true;
                return Err(io::Error::other("journal rotation worker disconnected"));
            }
        }
    }

    if !state.durability.rotation_requested && state.durability.writer.needs_rotation() {
        state.durability.rotation_requested = true;
    }
    if !state.durability.rotation_requested {
        return Ok(false);
    }

    if state.durability.pending.is_empty() && !state.durability.rotation_snapshot_ready {
        super::checkpoint::remember_drops_checkpoint(state)?;
        state.durability.rotation_snapshot_ready = true;
    }

    if state.durability.pending.is_empty()
        && state.durability.rotation_snapshot_ready
        && state.durability.dirty_checkpoints.is_empty()
        && state.durability.checkpoint_inflight.is_empty()
    {
        let sequence = state.durability.writer.sequence();
        let compacted_drops = state.drops.rotation_compaction();
        match state
            .durability
            .writer
            .try_rotate_with_drop_compaction(sequence, compacted_drops)
        {
            Ok(receiver) => state.durability.rotation_receipt = Some(receiver),
            Err(RotateError::Full) => {}
            Err(RotateError::Closed) => {
                state.durability.failed = true;
                return Err(io::Error::other("journal rotation queue is closed"));
            }
        }
    }
    Ok(true)
}
