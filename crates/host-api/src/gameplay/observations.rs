//! Advisory post-commit observations are read-only, not retryable decisions.
//! They may be dropped under pressure and are not replayed after restart.
use super::{Cell, Entity};
use crate::RegistrationError;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CommittedBlock {
    pub cell: Cell,
    pub state: String,
}

#[derive(Clone, Debug)]
pub enum CommittedEntity {
    Spawned(Entity),
    Updated(Entity),
    Removed { id: u64, key: String },
}

#[derive(Clone, Debug)]
pub struct Committed {
    pub blocks: Vec<CommittedBlock>,
    pub entities: Vec<CommittedEntity>,
    /// Profile and resulting revision, never private inventory contents.
    pub inventory: Option<(u128, u64)>,
}

#[derive(Clone, Copy, Debug)]
pub struct WeatherChanged {
    pub previous: super::Weather,
    pub current: super::Weather,
}

pub trait Observer: Send + Sync + 'static {
    /// Runs on a bounded, off-coordinator delivery lane. Cannot amend or veto
    /// the commit; a handler needing authoritative follow-up uses a scheduled
    /// durable decision rather than mutating process-global state here.
    fn on_commit(&self, event: &Committed);
    /// Advisory live target changes, including natural transitions. Not replayed.
    fn on_weather(&self, _event: &WeatherChanged) {}
}

#[derive(Clone)]
pub struct ObserverRegistration {
    pub key: String,
    pub version: u64,
    pub observer: Arc<dyn Observer>,
}
impl ObserverRegistration {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let Some((namespace, name)) = self.key.split_once(':') else {
            return Err(RegistrationError("invalid observer key".into()));
        };
        let valid = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        };
        if self.key.len() > 128 || !valid(namespace) || !valid(name) || self.version == 0 {
            return Err(RegistrationError("invalid observer declaration".into()));
        }
        Ok(())
    }
}
impl std::fmt::Debug for ObserverRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObserverRegistration")
            .field("key", &self.key)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}
