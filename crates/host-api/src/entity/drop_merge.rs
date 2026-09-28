//! Deterministic, bounded merge-target decision for authoritative item drops.
//! The host supplies a spatially bounded candidate set and persists the chosen
//! stack update, split, and allocator changes in its existing entity/WAL batch.

/// An existing or already-planned drop. `same_stack` means exact item and
/// component identity; count and age are supplied from authoritative state.
#[derive(Clone, Copy, Debug)]
pub struct DropMergeCandidate {
    pub id: u64,
    pub position: [f32; 3],
    pub count: u16,
    pub age_ms: u64,
    pub same_stack: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DropMergeContext {
    pub position: [f32; 3],
    pub radius: f32,
    pub lifetime_ms: u64,
    pub stack_limit: u16,
}

impl DropMergeContext {
    /// Choose the lowest eligible ID even if the captured candidates arrive in
    /// a different order. Equal-radius boundary and expired drops do not merge.
    pub fn select(self, candidates: impl IntoIterator<Item = DropMergeCandidate>) -> Option<u64> {
        if self.position.iter().any(|value| !value.is_finite())
            || !self.radius.is_finite()
            || self.radius <= 0.0
            || !(self.radius * self.radius).is_finite()
            || self.stack_limit == 0
        {
            return None;
        }
        let radius_sq = self.radius * self.radius;
        candidates
            .into_iter()
            .filter(|candidate| {
                candidate.id != 0
                    && candidate.same_stack
                    && candidate.count > 0
                    && candidate.count < self.stack_limit
                    && candidate.age_ms < self.lifetime_ms
                    && candidate.position.iter().all(|value| value.is_finite())
                    && candidate
                        .position
                        .iter()
                        .zip(self.position)
                        .map(|(coordinate, origin)| (coordinate - origin).powi(2))
                        .sum::<f32>()
                        < radius_sq
            })
            .map(|candidate| candidate.id)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_stable_lowest_eligible_id_without_crossing_cap_or_expiry() {
        let context = DropMergeContext {
            position: [0.5; 3],
            radius: 1.0,
            lifetime_ms: 3000,
            stack_limit: 128,
        };
        let candidate = |id, count, age_ms, same_stack, x| DropMergeCandidate {
            id,
            position: [x, 0.5, 0.5],
            count,
            age_ms,
            same_stack,
        };
        assert_eq!(
            context.select([
                candidate(7, 1, 0, true, 0.5),
                candidate(2, 128, 0, true, 0.5),
                candidate(1, 1, 3000, true, 0.5),
                candidate(3, 1, 0, false, 0.5),
                candidate(4, 1, 0, true, 1.5),
                candidate(5, 127, 2999, true, 0.5),
            ]),
            Some(5)
        );
        assert_eq!(context.select([candidate(7, 1, 0, false, 0.5)]), None);
    }
}
