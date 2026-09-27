//! Durable, non-archetypal mobile entities. Canonical bytes are the private
//! state; a bounded projection is the only data sent to clients/other handlers.
use crate::RegistrationError;
use std::sync::Arc;

pub trait EntityState: Send + Sync + 'static {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError>;
    fn public(&self, data: &[u8]) -> Result<Vec<u8>, RegistrationError>;
}

#[derive(Clone)]
pub struct EntityDefinition {
    pub key: String,
    pub schema_version: u16,
    pub schema_fingerprint: u64,
    pub max_state_bytes: u16,
    /// First due callback, in logical ticks after spawning. Subsequent due
    /// times are changed transactionally by gameplay handlers.
    pub initial_delay_ticks: Option<u32>,
    pub state: Arc<dyn EntityState>,
}
impl EntityDefinition {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let valid = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        };
        let Some((namespace, name)) = self.key.split_once(':') else {
            return Err(RegistrationError("invalid gameplay entity key".into()));
        };
        if self.key.len() > 128
            || !valid(namespace)
            || !valid(name)
            || self.schema_version == 0
            || !(1..=u16::MAX).contains(&self.max_state_bytes)
            || self
                .initial_delay_ticks
                .is_some_and(|ticks| !(1..=100_000).contains(&ticks))
        {
            return Err(RegistrationError(
                "invalid gameplay entity declaration".into(),
            ));
        }
        Ok(())
    }
}
impl std::fmt::Debug for EntityDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityDefinition")
            .field("key", &self.key)
            .field("schema_version", &self.schema_version)
            .finish_non_exhaustive()
    }
}
