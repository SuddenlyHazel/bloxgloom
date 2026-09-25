//! Client-side entity presentation registry.
//!
//! The server owns every entity simulation decision and replicates each type
//! through a bounded opaque public view (`PublicEntity`). This registry maps an
//! `EntityTypeId` to the small client behaviour that knows how to present that
//! view and which opaque interaction bytes to emit for it. Adding a new entity
//! type means adding one adapter (a new module plus a single `register` call
//! at startup) instead of editing the snapshot/commit assembler or the window
//! event dispatch.
//!
//! Presentation only: adapters receive `&PublicEntity` and `Hit` snapshots and
//! return owned visuals or owned `ClientMessage`s. They never see the player
//! inventory or the world-drop animator, so animation and presentation timing
//! cannot decide inventory or drop ownership; the server does.

use crate::content::{Catalog, EntityTypeId};
use crate::protocol::{ClientMessage, PublicEntity};
use crate::raycast::Hit;
use crate::render::VisualAvatar;
use std::collections::BTreeMap;

/// Interaction verb an adapter understands, e.g. `"kiln:insert-input"`.
///
/// Verbs are namespaced per entity type so key bindings and click handlers can
/// target behaviour without importing per-type command enums.
pub(in crate::client) type EntityVerb = &'static str;

/// Per-type client behaviour for one registered entity type.
///
/// Every function takes shared references and returns owned values, which is
/// what keeps presentation from influencing inventory or drop ownership.
pub(in crate::client) struct EntityAdapter {
    /// Entity type this adapter presents. Unknown ids simply have no adapter
    /// and degrade to "stored but not drawn" instead of panicking.
    pub entity_type: EntityTypeId,
    /// Project one public view to an avatar visual. `Ok(None)` means this
    /// entity has no avatar visual (normal for anchored types); `Err(())`
    /// means a known type arrived malformed and the snapshot must resync.
    pub project_avatar: fn(&PublicEntity) -> Result<Option<VisualAvatar>, ()>,
    /// Whether an aimed-block hit targets this adapter's entities.
    pub hit_test: fn(Hit, &Catalog) -> bool,
    /// Build the opaque interaction request for a handled hit and verb.
    /// Returns `None` when the verb is unknown to this adapter.
    pub interact: fn(Hit, u128, u8, EntityVerb) -> Option<ClientMessage>,
}

/// Maps `EntityTypeId` to presentation/interaction behaviour.
///
/// Registration order is hit-test priority for aimed-block interactions.
pub(in crate::client) struct EntityClientRegistry {
    adapters: Vec<EntityAdapter>,
}

impl EntityClientRegistry {
    pub(in crate::client) fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Builtin player + kiln presentation. New types register alongside these
    /// at startup without touching the assembler or event dispatch.
    pub(in crate::client) fn builtins() -> Self {
        let mut registry = Self::new();
        registry.register(super::avatar::player_adapter());
        registry.register(super::kiln::kiln_adapter());
        registry
    }

    pub(in crate::client) fn register(&mut self, adapter: EntityAdapter) {
        if let Some(existing) = self
            .adapters
            .iter_mut()
            .find(|entry| entry.entity_type == adapter.entity_type)
        {
            *existing = adapter;
        } else {
            self.adapters.push(adapter);
        }
    }

    fn adapter_for(&self, entity_type: EntityTypeId) -> Option<&EntityAdapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.entity_type == entity_type)
    }

    /// Project every entity with a registered adapter to avatar visuals.
    /// Entities without an adapter (unknown or future types) are skipped
    /// safely: they stay stored in the replica map, they just draw nothing.
    pub(in crate::client) fn project(
        &self,
        entities: &BTreeMap<u64, PublicEntity>,
    ) -> Result<Vec<VisualAvatar>, ()> {
        let mut avatars = Vec::new();
        for entity in entities.values() {
            let Some(adapter) = self.adapter_for(entity.entity_type) else {
                continue;
            };
            if let Some(avatar) = (adapter.project_avatar)(entity)? {
                avatars.push(avatar);
            }
        }
        Ok(avatars)
    }

    /// Whether any registered adapter handles an aimed-block hit.
    pub(in crate::client) fn handles(&self, hit: Hit, catalog: &Catalog) -> bool {
        self.adapters
            .iter()
            .any(|adapter| (adapter.hit_test)(hit, catalog))
    }

    /// Build the opaque interaction request for the first adapter handling
    /// the hit. `None` means no adapter handled the hit or the verb is
    /// unknown; the caller sends nothing in both cases.
    pub(in crate::client) fn interact(
        &self,
        hit: Hit,
        catalog: &Catalog,
        action_id: u128,
        hotbar_slot: u8,
        verb: EntityVerb,
    ) -> Option<ClientMessage> {
        for adapter in &self.adapters {
            if (adapter.hit_test)(hit, catalog) {
                return (adapter.interact)(hit, action_id, hotbar_slot, verb);
            }
        }
        None
    }
}

impl Default for EntityClientRegistry {
    fn default() -> Self {
        Self::new()
    }
}
