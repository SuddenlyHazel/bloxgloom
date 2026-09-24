//! Derived deadlines for bounded expiry checks and WAL batches.
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use super::LIFETIME;

#[derive(Clone, Default)]
pub(super) struct ExpiryIndex {
    deadlines: BTreeSet<(Instant, u64)>,
    deadline_by_id: HashMap<u64, Instant>,
}

impl ExpiryIndex {
    pub(super) fn insert(&mut self, id: u64, age: Duration, now: Instant) {
        self.remove(id);
        let deadline = now.checked_add(LIFETIME.saturating_sub(age)).unwrap_or(now);
        self.deadlines.insert((deadline, id));
        self.deadline_by_id.insert(id, deadline);
    }

    pub(super) fn remove(&mut self, id: u64) {
        if let Some(deadline) = self.deadline_by_id.remove(&id) {
            self.deadlines.remove(&(deadline, id));
        }
    }

    pub(super) fn has_expired(&self, now: Instant) -> bool {
        self.deadlines
            .first()
            .is_some_and(|(deadline, _)| *deadline <= now)
    }

    /// Selects the earliest due entries, then returns their IDs in stable order
    /// so WAL change ordering does not depend on deadline insertion order.
    pub(super) fn expired_ids(&self, now: Instant, limit: usize) -> Vec<u64> {
        let mut ids: Vec<_> = self
            .deadlines
            .range(..=(now, u64::MAX))
            .take(limit)
            .map(|(_, id)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.deadline_by_id.len()
    }

    #[cfg(test)]
    pub(super) fn contains(&self, id: u64) -> bool {
        self.deadline_by_id.contains_key(&id)
    }
}
