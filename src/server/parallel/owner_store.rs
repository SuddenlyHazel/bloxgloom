//! Coordinator-owned index of revisioned owner state.
//!
//! Workers receive cloned `Arc` snapshots only. A runtime captures a complete
//! wave, validates it, then calls `apply_validated` with exclusive access to
//! this store; all read revisions and typed replacement values are checked
//! before the first owner is updated.

use super::{OwnerKey, OwnerPatch, OwnerSnapshot, ValidatedOwnerWave};
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Typed extension-owned state shared immutably with worker snapshots.
///
/// This is runtime-local state only; persistence and generic effects are
/// separate contracts and are not implied by storing a value here.
#[derive(Clone)]
pub(in crate::server) struct OwnerData(Arc<dyn Any + Send + Sync>);

impl OwnerData {
    pub fn new<T: Any + Send + Sync>(value: T) -> Self {
        Self(Arc::new(value))
    }

    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.0.downcast_ref()
    }

    pub fn same_type(&self, other: &Self) -> bool {
        self.0.as_ref().type_id() == other.0.as_ref().type_id()
    }
}

struct OwnerCell<T> {
    revision: u64,
    value: Arc<T>,
}

/// Stable owner-to-snapshot index. The store itself stays with the runtime;
/// jobs only receive immutable snapshots and return replacement patches.
pub(in crate::server) struct OwnerStore<T> {
    owners: BTreeMap<OwnerKey, OwnerCell<T>>,
}

impl<T: Send + Sync + 'static> OwnerStore<T> {
    pub fn new() -> Self {
        Self {
            owners: BTreeMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        owner: OwnerKey,
        revision: u64,
        value: T,
    ) -> Result<(), OwnerStoreError> {
        if self.owners.contains_key(&owner) {
            return Err(OwnerStoreError::DuplicateOwner { owner });
        }
        self.owners.insert(
            owner,
            OwnerCell {
                revision,
                value: Arc::new(value),
            },
        );
        Ok(())
    }

    pub fn revision(&self, owner: OwnerKey) -> Option<u64> {
        self.owners.get(&owner).map(|cell| cell.revision)
    }

    pub fn len(&self) -> usize {
        self.owners.len()
    }

    pub fn snapshot(&self, owner: OwnerKey) -> Result<OwnerSnapshot, OwnerStoreError> {
        let cell = self
            .owners
            .get(&owner)
            .ok_or(OwnerStoreError::UnknownOwner { owner })?;
        Ok(OwnerSnapshot::new(
            owner,
            cell.revision,
            Arc::clone(&cell.value),
        ))
    }

    /// Captures snapshots in stable owner order and rejects missing owners.
    pub fn snapshots(
        &self,
        owners: impl IntoIterator<Item = OwnerKey>,
    ) -> Result<Vec<OwnerSnapshot>, OwnerStoreError> {
        let mut owners: Vec<_> = owners.into_iter().collect();
        owners.sort_unstable();
        for pair in owners.windows(2) {
            if pair[0] == pair[1] {
                return Err(OwnerStoreError::DuplicateOwner { owner: pair[0] });
            }
        }
        owners
            .into_iter()
            .map(|owner| self.snapshot(owner))
            .collect()
    }

    pub fn owners(&self) -> impl Iterator<Item = OwnerKey> + '_ {
        self.owners.keys().copied()
    }

    /// Replaces each owner with its typed patch value after rechecking every
    /// captured read revision. The mutable store borrow is the exclusive
    /// commit lease: no snapshot or competing wave can observe a partial apply.
    ///
    /// This is a focused reference store for runtime integration. A sharded
    /// production store can preserve the same preflight and revision contract
    /// while acquiring owner-local leases in canonical order.
    pub fn apply_validated(&mut self, wave: ValidatedOwnerWave) -> Result<usize, OwnerStoreError>
    where
        T: Clone,
    {
        self.apply_validated_with(wave, |patch: &OwnerPatch| {
            patch
                .payload::<T>()
                .cloned()
                .ok_or(OwnerStoreError::WrongPayloadType {
                    owner: patch.owner(),
                })
        })
    }

    /// Applies a validated wave after extracting each typed patch into its
    /// owner replacement value. The conversion runs during preflight and must
    /// not mutate external state.
    pub fn apply_validated_with(
        &mut self,
        wave: ValidatedOwnerWave,
        mut replacement_for: impl FnMut(&OwnerPatch) -> Result<T, OwnerStoreError>,
    ) -> Result<usize, OwnerStoreError> {
        let mut replacements = Vec::with_capacity(wave.patches().len());
        for patch in wave.patches() {
            let owner = patch.owner();
            let expected_primary = patch.key().snapshot_revision;
            if !patch
                .revisions()
                .iter()
                .any(|stamp| stamp.owner == owner && stamp.revision == expected_primary)
            {
                return Err(OwnerStoreError::MissingPrimaryRevision { owner });
            }
            for stamp in patch.revisions() {
                let actual = self.revision(stamp.owner);
                if actual != Some(stamp.revision) {
                    return Err(OwnerStoreError::StaleRevision {
                        owner: stamp.owner,
                        expected: stamp.revision,
                        actual,
                    });
                }
            }

            let Some(next_revision) = expected_primary.checked_add(1) else {
                return Err(OwnerStoreError::RevisionExhausted { owner });
            };
            let replacement = replacement_for(patch)?;
            replacements.push((owner, next_revision, Arc::new(replacement)));
        }

        // Every fallible condition is checked before this ordered apply pass.
        for (owner, next_revision, value) in &replacements {
            let cell = self
                .owners
                .get_mut(owner)
                .expect("owner revisions were validated before apply");
            cell.revision = *next_revision;
            cell.value = Arc::clone(value);
        }
        Ok(replacements.len())
    }
}

impl<T: Send + Sync + 'static> Default for OwnerStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::server) enum OwnerStoreError {
    DuplicateOwner {
        owner: OwnerKey,
    },
    UnknownOwner {
        owner: OwnerKey,
    },
    StaleRevision {
        owner: OwnerKey,
        expected: u64,
        actual: Option<u64>,
    },
    MissingPrimaryRevision {
        owner: OwnerKey,
    },
    WrongPayloadType {
        owner: OwnerKey,
    },
    InvalidReplacement {
        owner: OwnerKey,
        message: String,
    },
    RevisionExhausted {
        owner: OwnerKey,
    },
}

#[cfg(test)]
#[path = "owner_store/tests.rs"]
mod tests;
