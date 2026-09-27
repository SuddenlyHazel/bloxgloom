//! Host-only capture of exact stack equivalence, without copying private
//! component bytes into the immutable neighbour view dispatched to workers.
use super::{EntityError, EntityId, EntityView, MAX_PLAN_NEIGHBOURS};
use crate::server::entities::{store::EntityStore, transfer::AutomationStack};

impl EntityView {
    pub(in crate::server) fn inventory_policies(
        &mut self,
        store: &EntityStore,
        own: EntityId,
    ) -> Result<(), EntityError> {
        if self.entries.len() > MAX_PLAN_NEIGHBOURS {
            return Err(EntityError::PublicViewTooLarge);
        }
        // Bound capture, not just its output: at most 65 inventories of 54
        // slots, each holding <= 1024 component bytes. At most one exact value
        // per slot is retained in this temporary interner, which is dropped
        // before dispatch. Worker views retain only fixed-size opaque keys.
        // IDs are snapshot-local, assigned in deterministic own/peer/slot order;
        // they are not persisted or reused after restart or a replan.
        let mut identities = std::collections::BTreeMap::new();
        for id in std::iter::once(own).chain(self.entries.iter().map(|e| e.id)) {
            let Some(snapshot) = store.snapshot(id) else {
                continue;
            };
            if let Ok(descriptor) = store.types().descriptor(snapshot.entity_type)
                && let Some(policy) = descriptor.transfer_policy()
            {
                // These host codecs already enforce slot/component limits on
                // every admitted payload; no unbounded mod collection is read.
                let slots = policy.inventory_slots(&snapshot.private_payload)?;
                if slots.len() > 54 {
                    return Err(EntityError::InvalidPayload);
                }
                let projected = slots
                    .into_iter()
                    .map(|slot| {
                        slot.map(|s| {
                            let identity = (
                                s.item,
                                s.components.as_ref().map(|c| (c.version, c.bytes.clone())),
                            );
                            let next = identities.len() as u32;
                            let key = *identities.entry(identity).or_insert(next);
                            AutomationStack {
                                item: s.item,
                                count: s.count,
                                has_components: s.components.is_some(),
                                key,
                            }
                        })
                    })
                    .collect();
                self.automation.insert(id, projected);
                self.inventories.insert(id, std::sync::Arc::clone(policy));
            }
        }
        Ok(())
    }
}
