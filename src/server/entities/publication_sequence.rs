//! The persisted watermark orders publication, not conflicts. Production
//! finalizes the provisional planning transition at WAL admission. Receipts
//! and the checkpoint mirror consume that exact chain in admission order.
use super::*;

impl PreparedEntityTransaction {
    pub fn publication_frontier(&self) -> Result<(u64, u64), EntityError> {
        let change = self.publication_change()?;
        super::super::persistence::decode_revision_value(&change.after)
    }

    pub fn assign_publication(&mut self, before: (u64, u64)) -> Result<(), EntityError> {
        // The checkpoint format permits a live registry frontier ahead of its
        // last durable watermark. Preserve that gap on the first admission;
        // later in-flight records extend the admitted chain, not stale plans.
        let planned_revision = self.publication_frontier()?.1;
        let after = (
            before
                .0
                .checked_add(1)
                .ok_or(EntityError::RevisionExhausted)?,
            before
                .1
                .checked_add(1)
                .ok_or(EntityError::RevisionExhausted)?
                .max(planned_revision),
        );
        let change = self
            .changes
            .iter_mut()
            .find(|change| change.key.domain == ENTITY_REVISION_DOMAIN)
            .ok_or(EntityError::InvalidTransaction)?;
        change.before = encode_revision_value(before.0, before.1)?;
        change.after = encode_revision_value(after.0, after.1)?;
        Ok(())
    }

    fn publication_change(&self) -> Result<&Change, EntityError> {
        let mut changes = self
            .changes
            .iter()
            .filter(|change| change.key.domain == ENTITY_REVISION_DOMAIN);
        let change = changes.next().ok_or(EntityError::InvalidTransaction)?;
        if changes.next().is_some() || !change.key.bytes.is_empty() {
            return Err(EntityError::InvalidTransaction);
        }
        let before = super::super::persistence::decode_revision_value(&change.before)?;
        let after = super::super::persistence::decode_revision_value(&change.after)?;
        if before.0.checked_add(1) != Some(after.0) || after.1 <= before.1 {
            return Err(EntityError::InvalidTransaction);
        }
        Ok(change)
    }

    pub(super) fn validate_publication(&self, store: &EntityStore) -> Result<(), EntityError> {
        let change = self.publication_change()?;
        if change.before
            != encode_revision_value(store.durable_sequence, store.durable_global_revision)?
            || change.after
                != encode_revision_value(
                    store
                        .durable_sequence
                        .checked_add(1)
                        .ok_or(EntityError::RevisionExhausted)?,
                    store
                        .revision
                        .checked_add(1)
                        .ok_or(EntityError::RevisionExhausted)?,
                )?
        {
            return Err(EntityError::InvalidTransaction);
        }
        Ok(())
    }
}
