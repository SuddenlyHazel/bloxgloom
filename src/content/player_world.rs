//! Frozen transient region/chat rules; persistent profile schemas stay separate.
use super::Catalog;
use bloxgloom_host_api::{RegistrationError, chat, regions};
impl Catalog {
    pub(crate) fn register_region(
        &mut self,
        region: regions::Registration,
    ) -> Result<(), RegistrationError> {
        region.validate()?;
        if region.key.split_once(':').map(|p| p.0) != region.service.split_once(':').map(|p| p.0) {
            return Err(RegistrationError(
                "region service must belong to the declaring package".into(),
            ));
        }
        if self.regions.len() >= regions::MAX_REGIONS || self.regions.contains_key(&region.key) {
            return Err(RegistrationError("duplicate or over-limit region".into()));
        }
        self.regions.insert(region.key.clone(), region);
        Ok(())
    }
    pub(crate) fn regions(&self) -> impl Iterator<Item = &regions::Registration> {
        self.regions.values()
    }
    pub(crate) fn register_chat_hook(
        &mut self,
        hook: chat::Registration,
    ) -> Result<(), RegistrationError> {
        hook.validate()?;
        if self.chat_hooks.len() >= chat::MAX_HOOKS || self.chat_hooks.contains_key(&hook.key) {
            return Err(RegistrationError(
                "duplicate or over-limit chat hook".into(),
            ));
        }
        self.chat_hooks.insert(hook.key.clone(), hook);
        Ok(())
    }
    pub(crate) fn chat_hooks(&self) -> impl Iterator<Item = &chat::Registration> {
        self.chat_hooks.values()
    }
    pub(crate) fn validate_player_world_rules(&self) -> Result<(), RegistrationError> {
        for region in self.regions() {
            if !self
                .player_lifecycles()
                .any(|service| service.key == region.service)
            {
                return Err(RegistrationError(format!(
                    "{}: missing player lifecycle service {}",
                    region.key, region.service
                )));
            }
        }
        Ok(())
    }
}
