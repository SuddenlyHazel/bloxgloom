//! Fire retains its existing frontier/worker behavior. A burn of any registered
//! footprint is admitted with full removal/refunds in the same WAL record.
use super::*;

pub(super) fn touches_anchor(state: &State, transaction: &FireTransaction) -> bool {
    transaction.changed_cells.iter().any(|c| {
        state
            .entities
            .anchored_at(crate::server::entities::CellCoord::new(c.x, c.y, c.z))
            .is_some()
    })
}
pub(super) fn stage(
    state: &mut State,
    tick: TickId,
    mut transaction: FireTransaction,
) -> io::Result<()> {
    // The worker's before-chunk must still be current before rebuilding its
    // burn batch together with cross-chunk footprint cleanup.
    if let Some(edit) = &transaction.world_edit {
        state
            .world
            .validate_owner_apply_batch(std::slice::from_ref(edit))?;
    }
    let edits: Vec<_> = transaction
        .changed_cells
        .iter()
        .map(|c| (c.x, c.y, c.z, crate::world::AIR))
        .collect();
    let mut action = match actions::invalidation::plan(state, &edits, tick.get()) {
        Ok(Some(action)) => action,
        Ok(None) => return Err(io::Error::other("invalidation lost its anchor")),
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::QuotaExceeded) => {
            state.fire.note_conflict(1);
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let Some(permit) = state.durability.entity_mirror.try_reserve_durable()? else {
        state.fire.note_full(1);
        return Ok(());
    };
    if !action.terrain_reads.is_current() {
        state.fire.note_conflict(1);
        return Ok(());
    }
    let entities = action
        .entities
        .as_mut()
        .expect("invalidation has entity work");
    entities
        .assign_publication(state.durability.entity_publication_frontier)
        .map_err(io::Error::other)?;
    let frontier = entities.publication_frontier().map_err(io::Error::other)?;
    let reads = entities
        .read_keys()
        .cloned()
        .chain(action.terrain_reads.keys())
        .chain(action.changed_cells.iter().map(|c| {
            crate::server::entities::cell_state_key(crate::server::entities::CellCoord::new(
                c.x, c.y, c.z,
            ))
        }))
        .collect();
    // Action owns the new combined terrain edit and all replication. Fire owns
    // only its original durable frontier/mailboxes/cursor in this record.
    transaction.world_edit = None;
    transaction.changed_cells.clear();
    transaction
        .changes
        .retain(|c| c.key != chunk_state_key(transaction.owner));
    let mut changes = action_changes(&action, state.world.catalog())?;
    changes.extend(transaction.changes.iter().cloned());
    let bytes = changes.iter().fold(30usize, |sum, c| {
        sum.saturating_add(
            17 + c.key.domain.len() + c.key.bytes.len() + c.before.len() + c.after.len(),
        )
    });
    if bytes > crate::server::journal::MAX_TRANSACTION_BYTES {
        // Preserve the source. Backoff cannot make an oversized atomic removal
        // fit; it needs smaller content/local state, not a partial application.
        state.fire.note_full(1);
        return Ok(());
    }
    let mut payload = Some(PendingPayload::FireAction(transaction.clone(), action));
    match state
        .durability
        .try_stage_changes(tick, changes, reads, &mut payload, Some(permit))
    {
        Ok(true) => {
            state.durability.entity_publication_frontier = frontier;
            if let Err(error) = state.fire.mark_submitted(&transaction) {
                state.durability.failed = true;
                return Err(error);
            }
            state.fire.note_admitted(1);
        }
        Err(StageError::Full) => state.fire.note_full(1),
        Err(StageError::Conflict) => state.fire.note_conflict(1),
        other => {
            return Err(io::Error::other(format!(
                "fire lifecycle admission failed: {other:?}"
            )));
        }
    }
    Ok(())
}
