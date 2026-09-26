//! Derived read-only mobile pages for publication. Updated only when records
//! apply, rebuilt on load, never serialized. Capturing a region clones page
//! handles, not the world's entities or a page's population.
use super::store::EntityRecord;
use super::{EntityError, EntityId, EntityLocation, EntitySnapshot};
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(in crate::server) type MobilePage = Arc<BTreeMap<EntityId, EntitySnapshot>>;

#[derive(Default)]
pub(super) struct MobilePages(BTreeMap<ChunkKey, MobilePage>);

impl MobilePages {
    pub(super) fn replace(
        &mut self,
        before: Option<&EntityRecord>,
        after: Option<&EntityRecord>,
    ) -> Result<(), EntityError> {
        if let Some(before) = before.filter(|r| matches!(r.location, EntityLocation::Mobile { .. }))
        {
            let key = before.owner.chunk();
            let page = self
                .0
                .get_mut(&key)
                .ok_or(EntityError::InvalidTransaction)?;
            if Arc::make_mut(page).remove(&before.id).is_none() {
                return Err(EntityError::InvalidTransaction);
            }
            if page.is_empty() {
                self.0.remove(&key);
            }
        }
        if let Some(after) = after.filter(|r| matches!(r.location, EntityLocation::Mobile { .. })) {
            let page = self.0.entry(after.owner.chunk()).or_default();
            if Arc::make_mut(page)
                .insert(
                    after.id,
                    EntitySnapshot {
                        id: after.id,
                        entity_type: after.entity_type,
                        revision: after.revision,
                        motion_revision: after.motion_revision,
                        owner: after.owner,
                        location: after.location.clone(),
                        private_payload: after.payload.clone(),
                        next_tick: after.next_tick,
                    },
                )
                .is_some()
            {
                return Err(EntityError::InvalidTransaction);
            }
        }
        Ok(())
    }
    pub(super) fn page(&self, key: ChunkKey) -> Option<MobilePage> {
        self.0.get(&key).cloned()
    }
}
