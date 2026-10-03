//! Bounded server chat decisions. Moderation has no authority to mutate gameplay.
use crate::{RegistrationError, gameplay::Player};
use std::sync::Arc;
pub const MAX_TEXT_BYTES: usize = 512;
pub const MAX_HOOKS: usize = 32;
pub const MAX_RECIPIENTS: usize = 256;

pub fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= MAX_TEXT_BYTES && !text.chars().any(char::is_control)
}
#[derive(Clone, Debug)]
pub struct Request {
    pub sender: Player,
    pub text: String,
    /// Immutable admitted recipients; hooks cannot supply fabricated identities.
    pub players: Vec<Player>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    All,
    Sessions(Vec<(u128, u64)>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow { text: String, route: Route },
    Deny { reason: String },
}
pub trait Moderator: Send + Sync + 'static {
    fn moderate(&self, request: &Request) -> Result<Decision, String>;
}
#[derive(Clone)]
pub struct Registration {
    pub key: String,
    /// Frozen execution contract; scripts derive this from the declared revision,
    /// module entry and dependency contracts, excluding ordinary source edits.
    pub revision: u64,
    pub moderator: Arc<dyn Moderator>,
}
impl Registration {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        if !crate::regions::valid_key(&self.key) || self.revision == 0 {
            return Err(RegistrationError("invalid chat hook declaration".into()));
        }
        Ok(())
    }
}
impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration")
            .field("key", &self.key)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: u64,
    pub profile: u128,
    pub session: u64,
    pub name: String,
    pub text: String,
}

#[cfg(test)]
mod tests;
