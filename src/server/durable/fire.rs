//! Bounded WAL admission for worker-prepared chunk-owned fire.
//!
//! A deterministic, oldest-lane prefix of a validated candidate wave is
//! admitted as one bounded writer command. Each selected owner still has its
//! own atomic WAL record and crash-replay identity; deferred owners retain
//! their original frontier and cursor for a later tick.

use super::*;
use crate::server::fire::{FireTransaction, FireWave};
use crate::server::{State, streaming};

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
