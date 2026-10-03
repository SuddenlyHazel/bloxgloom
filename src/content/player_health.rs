//! Frozen, deterministic health policy and transaction hook declarations.
use super::Catalog;
use bloxgloom_host_api::{
    RegistrationError,
    player_health::{DamageRegistration, HookRegistration, MAX_HOOKS},
};
impl Catalog {
    pub(crate) fn register_damage_policy(
        &mut self,
        value: DamageRegistration,
    ) -> Result<(), RegistrationError> {
        value.validate()?;
        if self.damage_policies.len() >= MAX_HOOKS || self.damage_policies.contains_key(&value.key)
        {
            return Err(RegistrationError(
                "duplicate or over-limit damage policy".into(),
            ));
        }
        self.damage_policies.insert(value.key.clone(), value);
        Ok(())
    }
    pub(crate) fn register_health_hook(
        &mut self,
        value: HookRegistration,
    ) -> Result<(), RegistrationError> {
        value.validate()?;
        if self.health_hooks.len() >= MAX_HOOKS || self.health_hooks.contains_key(&value.key) {
            return Err(RegistrationError(
                "duplicate or over-limit health hook".into(),
            ));
        }
        self.health_hooks.insert(value.key.clone(), value);
        Ok(())
    }
    pub(crate) fn damage_policies(&self) -> impl Iterator<Item = &DamageRegistration> {
        self.damage_policies.values()
    }
    pub(crate) fn health_hooks(&self) -> impl Iterator<Item = &HookRegistration> {
        self.health_hooks.values()
    }
}
