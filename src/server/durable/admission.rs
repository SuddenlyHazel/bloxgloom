//! Reservation ownership shared by actions, owner waves and fire batches.
use super::*;

impl Durability {
    /// A rejected submission leaves the payload with its planner, consumes no
    /// transaction ID and holds no keys. Mirror permits cancel on drop before
    /// acceptance; afterwards the queue owns an armed permit until apply.
    pub(super) fn try_stage_changes(
        &mut self,
        tick: TickId,
        changes: Vec<crate::server::journal::Change>,
        read_keys: Vec<StateKey>,
        payload: &mut Option<PendingPayload>,
        mut entity_permit: Option<MirrorPermit>,
    ) -> Result<bool, StageError> {
        if changes.is_empty() {
            return Ok(false);
        }
        if self.failed {
            return Err(StageError::Closed);
        }
        if self.rotation_requested || self.pending.len() >= MAX_PENDING_DURABLE_ACTIONS {
            return Err(StageError::Full);
        }
        let mut keys = BTreeSet::new();
        for change in &changes {
            // Only publication metadata is chained at admission. Real
            // preimages, including owner and entity reads, are never rebased.
            if change.key.domain == crate::server::entities::ENTITY_REVISION_DOMAIN {
                continue;
            }
            if self.reserved.contains(&change.key) {
                return Err(StageError::Conflict);
            }
            keys.insert(change.key.clone());
        }
        let mut shared_read_keys = BTreeSet::new();
        for key in read_keys {
            if keys.contains(&key) {
                continue;
            }
            if self.reserved.contains(&key) && !self.shared_reads.contains_key(&key) {
                return Err(StageError::Conflict);
            }
            shared_read_keys.insert(key);
        }
        let mut projected_checkpoint_bytes: HashMap<StateKey, usize> = self
            .dirty_checkpoints
            .iter()
            .map(|(key, checkpoint)| (key.clone(), checkpoint.snapshot.len()))
            .collect();
        for pending in &self.pending {
            for (key, size) in &pending.checkpoint_sizes {
                let entry = projected_checkpoint_bytes.entry(key.clone()).or_default();
                *entry = (*entry).max(*size);
            }
        }
        for change in &changes {
            if is_checkpoint_key(&change.key) {
                projected_checkpoint_bytes.insert(change.key.clone(), change.after.len());
            }
        }
        let projected_bytes = projected_checkpoint_bytes
            .values()
            .copied()
            .fold(0usize, usize::saturating_add);
        if projected_checkpoint_bytes.len() > MAX_DIRTY_CHECKPOINT_KEYS
            || projected_bytes > MAX_DIRTY_CHECKPOINT_BYTES
        {
            return Err(StageError::Full);
        }
        let checkpoint_sizes = changes
            .iter()
            .filter(|change| is_checkpoint_key(&change.key))
            .map(|change| (change.key.clone(), change.after.len()))
            .collect();
        let id = self.next_id;
        let next_id = id.checked_add(1).ok_or(StageError::IdExhausted)?;
        let receiver = self
            .writer
            .try_submit(Transaction::new(id, tick.get(), changes))
            .map_err(|error| match error {
                SubmitError::Full => StageError::Full,
                SubmitError::Closed => StageError::Closed,
                SubmitError::Invalid(error) => StageError::Invalid(error),
            })?;
        if let Some(permit) = &mut entity_permit
            && let Err(error) = permit.mark_authoritative_change()
        {
            self.failed = true;
            return Err(StageError::Invalid(error));
        }
        self.reserved.extend(keys.iter().cloned());
        for key in &shared_read_keys {
            *self.shared_reads.entry(key.clone()).or_default() += 1;
            self.reserved.insert(key.clone());
        }
        self.next_id = next_id;
        self.pending.push(PendingCommit {
            id,
            receiver,
            submitted_at: Instant::now(),
            keys: keys.into_iter().collect(),
            shared_read_keys: shared_read_keys.into_iter().collect(),
            checkpoint_sizes,
            payload: payload.take().expect("admitted payload consumed once"),
            entity_permit,
        });
        Ok(true)
    }

    /// Only confirmed apply releases reservations. Shared reads fence every
    /// writer (including fire) until the last accepted reader has applied.
    /// Failed accepted work is quarantined behind `failed` until restart.
    pub(super) fn release_reservations(&mut self, keys: Vec<StateKey>, reads: Vec<StateKey>) {
        for key in reads {
            let readers = self
                .shared_reads
                .get_mut(&key)
                .expect("pending shared read reservation");
            *readers -= 1;
            if *readers == 0 {
                self.shared_reads.remove(&key);
                self.reserved.remove(&key);
            }
        }
        for key in keys {
            self.reserved.remove(&key);
        }
    }
}
