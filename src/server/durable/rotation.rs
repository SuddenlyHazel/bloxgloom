//! Checkpoint-gated journal generation rotation.

use super::*;
use crate::server::State;

/// Admissions stop at the WAL soft cap; already accepted transactions drain
/// and become visible before the fenced entity checkpoint and all dirty
/// BGED/BGIN files are fenced.
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
                let ticket = state
                    .durability
                    .entity_checkpoint_ticket
                    .take()
                    .ok_or_else(|| {
                        io::Error::other("journal rotated without a fenced entity checkpoint")
                    })?;
                state
                    .durability
                    .entity_mirror
                    .finish_checkpoint_fence(ticket)?;
                state.durability.rotation_receipt = None;
                state.durability.rotation_requested = false;
                state.durability.rotation_snapshot_ready = false;
                state.durability.completed_rotations = state
                    .durability
                    .completed_rotations
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("rotation count exhausted"))?;
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
    if state
        .durability
        .force_rotation_at_sequence
        .is_some_and(|sequence| state.durability.writer.sequence() >= sequence)
    {
        state.durability.force_rotation_at_sequence = None;
        state.durability.rotation_requested = true;
    }
    if !state.durability.rotation_requested {
        return Ok(false);
    }

    if state.durability.pending.is_empty() && !state.durability.rotation_snapshot_ready {
        let Some(ticket) = state.durability.entity_mirror.try_begin_checkpoint()? else {
            return Ok(true);
        };
        state.durability.entity_checkpoint_ticket = Some(ticket);
        state.durability.rotation_snapshot_ready = true;
    }

    let entity_checkpoint_ready =
        if let Some(ticket) = state.durability.entity_checkpoint_ticket.as_mut() {
            match state.durability.entity_mirror.poll_checkpoint(ticket)? {
                Some(receipt) => {
                    if receipt.durable_sequence != state.entities.durable_sequence()
                        || receipt.registry_revision != state.entities.revision()
                    {
                        state.durability.failed = true;
                        return Err(io::Error::other(
                            "entity checkpoint does not match WAL-committed entity frontier",
                        ));
                    }
                    true
                }
                None => false,
            }
        } else {
            false
        };

    if state.durability.pending.is_empty()
        && state.durability.rotation_snapshot_ready
        && entity_checkpoint_ready
        && state.durability.dirty_checkpoints.is_empty()
        && state.durability.checkpoint_inflight.is_empty()
    {
        let sequence = state.durability.writer.sequence();
        match state.durability.writer.try_rotate(sequence) {
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
