//! Bounded WAL admission for worker-prepared chunk-owned fire.
//!
//! A deterministic, oldest-lane prefix of a validated candidate wave is
//! admitted as one bounded writer command. Each selected owner still has its
//! own atomic WAL record and crash-replay identity; deferred owners retain
//! their original frontier and cursor for a later tick.

use super::*;
use crate::server::fire::{FireTransaction, FireWave};
use crate::server::{State, streaming};
#[path = "fire/invalidation.rs"]
mod invalidation;

impl Durability {
    pub(super) fn try_stage_fire_wave(
        &mut self,
        tick: TickId,
        transactions: &[FireTransaction],
    ) -> Result<bool, StageError> {
        if transactions.is_empty() {
            return Ok(false);
        }
        if self.failed {
            return Err(StageError::Closed);
        }
        if self.rotation_requested
            || self.pending.len().saturating_add(transactions.len()) > MAX_PENDING_DURABLE_ACTIONS
        {
            return Err(StageError::Full);
        }

        let mut projected_sizes: HashMap<StateKey, usize> = self
            .dirty_checkpoints
            .iter()
            .map(|(key, dirty)| (key.clone(), dirty.snapshot.len()))
            .collect();
        for commit in &self.pending {
            for (key, size) in &commit.checkpoint_sizes {
                let entry = projected_sizes.entry(key.clone()).or_default();
                *entry = (*entry).max(*size);
            }
        }
        let mut all_keys = HashSet::<StateKey>::new();
        let mut planned = Vec::with_capacity(transactions.len());
        let mut keys_per_record = Vec::with_capacity(transactions.len());
        let mut checkpoints_per_record = Vec::with_capacity(transactions.len());
        let mut next_id = self.next_id;
        for transaction in transactions {
            let changes = transaction.changes().to_vec();
            if changes.is_empty() {
                return Err(StageError::Invalid(io::Error::new(
                    ErrorKind::InvalidInput,
                    "fire owner emitted an empty WAL record",
                )));
            }
            let mut keys = Vec::with_capacity(changes.len());
            let mut checkpoint_sizes = HashMap::new();
            for change in &changes {
                if self.reserved.contains(&change.key) {
                    return Err(StageError::Conflict);
                }
                if !all_keys.insert(change.key.clone()) {
                    return Err(StageError::Invalid(io::Error::new(
                        ErrorKind::InvalidData,
                        "fire wave contains conflicting owner keys",
                    )));
                }
                keys.push(change.key.clone());
                if is_checkpoint_key(&change.key) {
                    projected_sizes.insert(change.key.clone(), change.after.len());
                    checkpoint_sizes.insert(change.key.clone(), change.after.len());
                }
            }
            planned.push(Transaction::new(next_id, tick.get(), changes));
            next_id = next_id.checked_add(1).ok_or(StageError::IdExhausted)?;
            keys_per_record.push(keys);
            checkpoints_per_record.push(checkpoint_sizes);
        }
        if projected_sizes.len() > MAX_DIRTY_CHECKPOINT_KEYS
            || projected_sizes
                .values()
                .copied()
                .fold(0usize, usize::saturating_add)
                > MAX_DIRTY_CHECKPOINT_BYTES
        {
            return Err(StageError::Full);
        }
        let receivers = self
            .writer
            .try_submit_batch(planned)
            .map_err(|error| match error {
                SubmitError::Full => StageError::Full,
                SubmitError::Closed => StageError::Closed,
                SubmitError::Invalid(error) => StageError::Invalid(error),
            })?;
        self.next_id = next_id;
        self.reserved.extend(all_keys);
        for (index, (((transaction, receiver), keys), checkpoint_sizes)) in transactions
            .iter()
            .cloned()
            .zip(receivers)
            .zip(keys_per_record)
            .zip(checkpoints_per_record)
            .enumerate()
        {
            self.pending.push(PendingCommit {
                id: self.next_id - transactions.len() as u128 + index as u128,
                shared_read_keys: Vec::new(),
                receiver,
                submitted_at: Instant::now(),
                keys,
                checkpoint_sizes,
                payload: PendingPayload::Fire(transaction),
                entity_permit: None,
            });
        }
        Ok(true)
    }
}

pub(in crate::server) fn stage_wave(
    state: &mut State,
    tick: TickId,
    wave: FireWave,
) -> io::Result<()> {
    for key in wave.missing_chunks {
        // A full loader is ordinary backpressure. Persistent source/delivery
        // records remain scheduled and the request is retried next tick.
        let _ = streaming::request_chunk(state, key)?;
    }
    let mut transactions = wave.transactions;
    if transactions.is_empty() {
        return Ok(());
    }
    state.fire.prioritize_transactions(&mut transactions);
    // Preserve canonical fire order. Ordinary waves retain the existing batched
    // admission; a mixed wave admits each exceptional footprint burn as one combined
    // record. Deferred removals retain their original frontier for retry.
    if transactions
        .iter()
        .any(|t| invalidation::touches_anchor(state, t))
    {
        for transaction in transactions {
            if invalidation::touches_anchor(state, &transaction) {
                invalidation::stage(state, tick, transaction)?;
            } else if transaction.world_edit.is_some() {
                stage_gameplay_burn(state, tick, transaction)?;
            } else {
                stage_transactions(state, tick, vec![transaction])?;
            }
        }
        return Ok(());
    }
    if transactions.iter().any(|t| t.world_edit.is_some()) {
        for transaction in transactions {
            if transaction.world_edit.is_some() {
                stage_gameplay_burn(state, tick, transaction)?;
            } else {
                stage_transactions(state, tick, vec![transaction])?;
            }
        }
        return Ok(());
    }
    stage_transactions(state, tick, transactions)
}

fn stage_gameplay_burn(
    state: &mut State,
    tick: TickId,
    mut transaction: FireTransaction,
) -> io::Result<()> {
    let mut action = match actions::gameplay_fire::plan(state, &transaction, tick.get()) {
        Ok(action) => action,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::WouldBlock | ErrorKind::QuotaExceeded
            ) =>
        {
            state.fire.note_conflict(1);
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    if !action.terrain_reads.is_current() || !action.terrain_reads.entities_current(&state.entities)
    {
        state.fire.note_conflict(1);
        return Ok(());
    }
    let permit = if let Some(entities) = &mut action.entities {
        let Some(permit) = state.durability.entity_mirror.try_reserve_durable()? else {
            state.fire.note_full(1);
            return Ok(());
        };
        entities
            .assign_publication(state.durability.entity_publication_frontier)
            .map_err(io::Error::other)?;
        Some((
            permit,
            entities.publication_frontier().map_err(io::Error::other)?,
        ))
    } else {
        None
    };
    let reads = action
        .entities
        .as_ref()
        .into_iter()
        .flat_map(|entities| entities.read_keys().cloned())
        .chain(action.terrain_reads.keys())
        .chain(action.changed_cells.iter().map(|c| {
            crate::server::entities::cell_state_key(crate::server::entities::CellCoord::new(
                c.x, c.y, c.z,
            ))
        }))
        .collect();
    transaction.world_edit = None;
    transaction.changed_cells.clear();
    transaction
        .changes
        .retain(|change| change.key != chunk_state_key(transaction.owner));
    let mut changes = action_changes(&action, state.world.catalog())?;
    changes.extend(transaction.changes.iter().cloned());
    let bytes = changes.iter().fold(30usize, |sum, change| {
        sum.saturating_add(
            17 + change.key.domain.len()
                + change.key.bytes.len()
                + change.before.len()
                + change.after.len(),
        )
    });
    if bytes > crate::server::journal::MAX_TRANSACTION_BYTES {
        state.fire.note_full(1);
        return Ok(());
    }
    let (permit, frontier) = match permit {
        Some((permit, frontier)) => (Some(permit), Some(frontier)),
        None => (None, None),
    };
    let mut payload = Some(PendingPayload::FireAction(transaction.clone(), action));
    match state
        .durability
        .try_stage_changes(tick, changes, reads, &mut payload, permit)
    {
        Ok(true) => {
            if let Some(frontier) = frontier {
                state.durability.entity_publication_frontier = frontier;
            }
            if let Err(error) = state.fire.mark_submitted(&transaction) {
                state.durability.failed = true;
                return Err(error);
            }
            state.fire.note_admitted(1);
        }
        Err(StageError::Full) => state.fire.note_full(1),
        Err(StageError::Conflict) => state.fire.note_conflict(1),
        other => {
            state.durability.failed = true;
            return Err(io::Error::other(format!(
                "fire gameplay WAL admission failed: {other:?}"
            )));
        }
    }
    Ok(())
}

fn stage_transactions(
    state: &mut State,
    tick: TickId,
    transactions: Vec<FireTransaction>,
) -> io::Result<()> {
    let mut prefix = transactions.len();
    let mut deferred_reason = None;
    loop {
        match state
            .durability
            .try_stage_fire_wave(tick, &transactions[..prefix])
        {
            Ok(true) => {
                state.fire.note_admitted(prefix);
                for transaction in &transactions[..prefix] {
                    if let Err(error) = state.fire.mark_submitted(transaction) {
                        state.durability.failed = true;
                        return Err(io::Error::other(format!(
                            "accepted fire wave could not reserve owner: {error}"
                        )));
                    }
                }
                if prefix < transactions.len() {
                    match deferred_reason.expect("a prefix was reduced after backpressure") {
                        StageError::Conflict => {
                            state.fire.note_conflict(transactions.len() - prefix)
                        }
                        StageError::Full => state.fire.note_full(transactions.len() - prefix),
                        _ => unreachable!("only retryable admission errors reduce the prefix"),
                    }
                }
                return Ok(());
            }
            Ok(false) => {
                state.durability.failed = true;
                return Err(io::Error::other("nonempty fire wave was not admitted"));
            }
            Err(reason @ (StageError::Conflict | StageError::Full)) => {
                if prefix == 1 || state.durability.rotation_requested {
                    match reason {
                        StageError::Conflict => state.fire.note_conflict(transactions.len()),
                        StageError::Full => state.fire.note_full(transactions.len()),
                        _ => unreachable!(),
                    }
                    return Ok(());
                }
                deferred_reason = Some(reason);
                prefix = prefix.div_ceil(2);
            }
            Err(error) => {
                state.durability.failed = true;
                return Err(io::Error::other(format!(
                    "fire WAL admission failed: {error:?}"
                )));
            }
        }
    }
}

pub(in crate::server) fn run_source(state: &mut State, tick: TickId) -> io::Result<()> {
    let wave = state
        .fire
        .prepare_source_wave(&mut state.world, &state.phase_plan, tick)?;
    stage_wave(state, tick, wave)
}

pub(in crate::server) fn run_delivery(state: &mut State, tick: TickId) -> io::Result<()> {
    let wave = state
        .fire
        .prepare_delivery_wave(&mut state.world, &state.phase_plan, tick)?;
    stage_wave(state, tick, wave)
}

/// Called only for a contiguous sequence of successful WAL receipts. All
/// fire reservations and owner revisions are checked against the same
/// pre-apply state before any owner worker can install an after-chunk. The
/// owner workers finish before frontier, checkpoint, or client publication.
pub(super) fn apply_synced_batch(
    state: &mut State,
    mut transactions: Vec<FireTransaction>,
) -> io::Result<()> {
    state.fire.validate_synced_batch(&transactions)?;
    let edits = transactions
        .iter_mut()
        .filter_map(|transaction| transaction.world_edit.take())
        .collect();
    state
        .fire
        .apply_synced_world_edits(&mut state.world, edits)?;
    for transaction in transactions {
        super::publication::publish_committed_fire_after_world(state, transaction)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "fire/tests.rs"]
mod tests;
