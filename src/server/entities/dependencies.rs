//! Bounded query dependencies. Empty pages protect absence; record revisions
//! protect member contents and positions without serializing unrelated writers.
use super::*;

#[derive(Clone, Debug, Default)]
pub struct EntityDependencies {
    chunks: BTreeMap<ChunkKey, super::super::spatial::ChunkPage>,
    records: BTreeMap<EntityId, (u64, u64)>,
}

impl EntityDependencies {
    pub fn is_current(&self, store: &EntityStore) -> bool {
        self.chunks
            .iter()
            .all(|(key, page)| match store.indexes.chunks.get(key) {
                Some(current) => current == page,
                None => page.entity_ids.is_empty(),
            })
            && self.records.iter().all(|(id, expected)| {
                store
                    .records
                    .get(id)
                    .is_some_and(|record| (record.revision, record.motion_revision) == *expected)
            })
    }

    pub(super) fn merge(&mut self, other: Self) -> Result<(), EntityError> {
        for (key, page) in other.chunks {
            if self.chunks.get(&key).is_some_and(|old| *old != page) {
                return Err(EntityError::InvalidTransaction);
            }
            self.chunks.insert(key, page);
        }
        for (id, revisions) in other.records {
            if self.records.get(&id).is_some_and(|old| *old != revisions) {
                return Err(EntityError::InvalidTransaction);
            }
            self.records.insert(id, revisions);
        }
        let references: usize = self.chunks.values().map(|page| page.entity_ids.len()).sum();
        if self.chunks.len() + references + 2 * self.records.len() > MAX_ENTITY_TRANSACTION_CHANGES
        {
            return Err(EntityError::TooManyTransactionChanges);
        }
        Ok(())
    }

    fn keys(&self) -> impl Iterator<Item = StateKey> + '_ {
        self.chunks.keys().copied().map(chunk_state_key).chain(
            self.records
                .keys()
                .flat_map(|id| [entity_state_key(*id), motion_state_key(*id)]),
        )
    }
}

impl EntityStore {
    /// AABB decisions must fence all containing owner pages, not just the IDs
    /// returned by the mobile index: a previously absent candidate can enter.
    pub fn capture_mobile_dependencies(
        &self,
        position: [f32; 3],
        radius: f32,
    ) -> Result<EntityDependencies, EntityError> {
        if !radius.is_finite() || radius < 0.0 {
            return Err(EntityError::InvalidLocation);
        }
        let min = position_to_cell(position.map(|value| value - radius))?.chunk();
        let max = position_to_cell(position.map(|value| value + radius))?.chunk();
        let volume = (i64::from(max.x) - i64::from(min.x) + 1)
            .checked_mul(i64::from(max.y) - i64::from(min.y) + 1)
            .and_then(|n| n.checked_mul(i64::from(max.z) - i64::from(min.z) + 1));
        if volume.is_none_or(|n| n > MAX_ENTITY_TRANSACTION_CHANGES as i64) {
            return Err(EntityError::SpatialQueryTooBroad);
        }
        let chunks = (min.x..=max.x).flat_map(|x| {
            (min.y..=max.y).flat_map(move |y| (min.z..=max.z).map(move |z| ChunkKey { x, y, z }))
        });
        self.capture_dependencies(chunks, MAX_ENTITY_TRANSACTION_CHANGES / 2)
    }

    /// Capture whole pages, never a query prefix. Check the policy bound and
    /// transaction-key bound before cloning or traversing each page.
    pub fn capture_dependencies(
        &self,
        chunks: impl IntoIterator<Item = ChunkKey>,
        maximum_references: usize,
    ) -> Result<EntityDependencies, EntityError> {
        let mut result = EntityDependencies::default();
        let mut references = 0usize;
        for (examined, chunk) in chunks.into_iter().enumerate() {
            if examined >= MAX_ENTITY_TRANSACTION_CHANGES {
                return Err(EntityError::SpatialQueryTooBroad);
            }
            if result.chunks.contains_key(&chunk) {
                continue;
            }
            let page = self.indexes.chunks.get(&chunk);
            references = references.saturating_add(page.map_or(0, |page| page.entity_ids.len()));
            if page.is_some_and(|page| page.entity_ids.len() > maximum_references)
                || result.chunks.len() + 1 + 3 * references > MAX_ENTITY_TRANSACTION_CHANGES
            {
                return Err(EntityError::SpatialQueryTooBroad);
            }
            if let Some(page) = page {
                for id in &page.entity_ids {
                    let record = self
                        .records
                        .get(id)
                        .ok_or(EntityError::InvalidTransaction)?;
                    result
                        .records
                        .insert(*id, (record.revision, record.motion_revision));
                }
            }
            if result.records.len() > maximum_references {
                return Err(EntityError::SpatialQueryTooBroad);
            }
            result
                .chunks
                .insert(chunk, page.cloned().unwrap_or_default());
        }
        Ok(result)
    }
}

impl PreparedEntityTransaction {
    pub fn add_dependencies(
        &mut self,
        dependencies: EntityDependencies,
    ) -> Result<(), EntityError> {
        let mut combined = self.dependencies.clone();
        combined.merge(dependencies)?;
        let writes: BTreeSet<_> = self.changes.iter().map(|change| &change.key).collect();
        let keys: BTreeSet<_> = self
            .additional_read_keys
            .iter()
            .cloned()
            .chain(combined.keys())
            .filter(|key| !writes.contains(key))
            .collect();
        if keys.len() + self.changes.len() > MAX_ENTITY_TRANSACTION_CHANGES {
            return Err(EntityError::TooManyTransactionChanges);
        }
        self.additional_read_keys = keys.into_iter().collect();
        self.dependencies = combined;
        Ok(())
    }
}
