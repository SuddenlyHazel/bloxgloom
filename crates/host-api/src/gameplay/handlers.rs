use super::{Block, Cell, Context, Error};
use crate::RegistrationError;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventKind {
    BlockRemoved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemovalCause {
    Break,
    Replacement,
    SupportLoss,
}

#[derive(Clone, Debug)]
pub enum Event {
    BlockRemoved {
        cell: Cell,
        previous: Block,
        cause: RemovalCause,
        random: u64,
    },
}

/// Pure/retryable gameplay decision. Staged operations are committed together
/// only when the handler succeeds and every dependency is still current.
pub trait Handler: Send + Sync + 'static {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error>;
}

#[derive(Clone)]
pub struct HandlerRegistration {
    pub key: String,
    /// Bump when changing native behavior; scripts use package content identity.
    pub version: u64,
    pub event: EventKind,
    /// An exact block type owns its decision; None is the fallback for types
    /// without an exact handler. Duplicate owners are registration errors.
    pub target: Option<String>,
    pub handler: Arc<dyn Handler>,
}

impl HandlerRegistration {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        fn key(s: &str) -> bool {
            let Some((namespace, name)) = s.split_once(':') else {
                return false;
            };
            s.len() <= 128
                && !namespace.is_empty()
                && !name.is_empty()
                && namespace
                    .bytes()
                    .chain(name.bytes())
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        }
        if !key(&self.key) || self.target.as_deref().is_some_and(|s| !key(s)) {
            return Err(RegistrationError(
                "invalid gameplay handler key or target".into(),
            ));
        }
        Ok(())
    }

    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut bytes = self.version.to_le_bytes().to_vec();
        bytes.push(match self.event {
            EventKind::BlockRemoved => 0,
        });
        bytes.push(u8::from(self.target.is_some()));
        if let Some(target) = &self.target {
            bytes.extend(target.as_bytes());
        }
        bytes
    }
}

impl std::fmt::Debug for HandlerRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerRegistration")
            .field("key", &self.key)
            .field("version", &self.version)
            .field("event", &self.event)
            .field("target", &self.target)
            .finish_non_exhaustive()
    }
}
