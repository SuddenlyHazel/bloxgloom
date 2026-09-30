//! Frozen player lifecycle registrations and their content identities.
use super::Catalog;
use bloxgloom_host_api::{RegistrationError, players::Registration};
use std::sync::Arc;
impl Catalog {
    pub(crate) fn player_authority(&self, namespace: &str) -> bool {
        self.composition
            .permits(namespace, bloxgloom_host_api::composition::PLAYERS)
    }
    pub(crate) fn player_delivery_enabled(&self) -> bool {
        !self.player_lifecycles.is_empty()
            || self
                .composition
                .requires(bloxgloom_host_api::composition::PLAYERS)
    }
    pub(crate) fn player_service_keys(&self) -> std::collections::BTreeSet<&str> {
        self.player_lifecycles
            .values()
            .map(|r| r.key.as_str())
            .chain(
                self.client_metadata
                    .identities
                    .iter()
                    .filter(|((kind, _), _)| *kind == b'Q')
                    .map(|(_, (key, _))| key.as_str()),
            )
            .collect()
    }

    pub(crate) fn register_player_lifecycle(
        &mut self,
        registration: Registration,
    ) -> Result<(), RegistrationError> {
        registration.validate()?;
        if self.player_lifecycles.len() >= 128
            || self
                .player_lifecycles
                .values()
                .any(|old| old.key == registration.key)
            || self
                .owner_systems
                .values()
                .any(|old| old.key == registration.key)
        {
            return Err(RegistrationError(
                "duplicate or over-limit player lifecycle".into(),
            ));
        }
        let id = self
            .player_lifecycles
            .keys()
            .next_back()
            .map_or(0, |id| id + 1);
        self.player_lifecycles.insert(id, Arc::new(registration));
        Ok(())
    }
    pub(crate) fn player_lifecycles(&self) -> impl Iterator<Item = &Arc<Registration>> {
        self.player_lifecycles.values()
    }
}
