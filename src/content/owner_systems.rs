use super::Catalog;
use bloxgloom_host_api::{RegistrationError, system::System};
use std::sync::Arc;
impl Catalog {
    pub(crate) fn owner_systems(&self) -> impl Iterator<Item = &Arc<System>> {
        self.owner_systems.values()
    }
    pub(crate) fn register_owner_system(
        &mut self,
        system: System,
    ) -> Result<(), RegistrationError> {
        system.validate()?;
        if self.owner_systems().any(|old| old.key == system.key)
            || self.player_lifecycles().any(|old| old.key == system.key)
        {
            return Err(RegistrationError(format!(
                "{}: duplicate owner system/lifecycle key",
                system.key
            )));
        }
        if self.owner_systems.len() >= 128 {
            return Err(RegistrationError(format!(
                "{} owner systems/installation: attempted {}; maximum 128",
                system.key,
                self.owner_systems.len() + 1
            )));
        }
        let id = self.owner_systems.keys().next_back().map_or(0, |id| id + 1);
        self.owner_systems.insert(id, Arc::new(system));
        Ok(())
    }
}
