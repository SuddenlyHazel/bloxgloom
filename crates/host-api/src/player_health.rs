//! Server-owned durable health. No client can supply a damage outcome.
use crate::{
    RegistrationError,
    gameplay::{Context, Error, Player},
};
use std::sync::Arc;
pub const PROFILE_SYSTEM: &str = "bloxgloom:player_health";
pub const MAX_HEALTH: u32 = 1_000_000;
pub const MAX_HOOKS: usize = 32;
pub const STATE_BYTES: usize = 37;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub current: u32,
    pub max: u32,
    pub life: u64,
    pub respawn: Option<(u64, [f32; 3])>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            current: 100,
            max: 100,
            life: 1,
            respawn: None,
        }
    }
}
impl State {
    pub fn alive(self) -> bool {
        self.current != 0
    }
    pub fn validate(self) -> Result<(), RegistrationError> {
        if self.max == 0 || self.max > MAX_HEALTH || self.current > self.max || self.life == 0 {
            return Err(RegistrationError("invalid health state".into()));
        }
        if let Some((life, position)) = self.respawn {
            if life == 0
                || life > self.life
                || position
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() >= 1_000_000.)
            {
                return Err(RegistrationError(
                    "invalid durable respawn checkpoint".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn encode(self) -> Result<Vec<u8>, RegistrationError> {
        self.validate()?;
        let mut bytes = vec![1];
        bytes.extend(self.current.to_le_bytes());
        bytes.extend(self.max.to_le_bytes());
        bytes.extend(self.life.to_le_bytes());
        let (life, position) = self.respawn.unwrap_or((0, [0.; 3]));
        bytes.extend(life.to_le_bytes());
        for axis in position {
            bytes.extend(axis.to_le_bytes());
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, RegistrationError> {
        if bytes.len() != STATE_BYTES || bytes[0] != 1 {
            return Err(RegistrationError("invalid health encoding".into()));
        }
        let value = Self {
            current: u32::from_le_bytes(bytes[1..5].try_into().unwrap()),
            max: u32::from_le_bytes(bytes[5..9].try_into().unwrap()),
            life: u64::from_le_bytes(bytes[9..17].try_into().unwrap()),
            respawn: {
                let life = u64::from_le_bytes(bytes[17..25].try_into().unwrap());
                let position = std::array::from_fn(|i| {
                    f32::from_le_bytes(bytes[25 + i * 4..29 + i * 4].try_into().unwrap())
                });
                if life == 0 {
                    if position != [0.; 3] {
                        return Err(RegistrationError("noncanonical respawn checkpoint".into()));
                    }
                    None
                } else {
                    Some((life, position))
                }
            },
        };
        value.validate()?;
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub current: u32,
    pub max: u32,
    pub alive: bool,
    pub revision: u64,
    pub life: u64,
}
impl View {
    pub fn new(state: State, revision: u64) -> Self {
        Self {
            current: state.current,
            max: state.max,
            alive: state.alive(),
            revision,
            life: state.life,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Damage {
    pub target: Player,
    pub health: View,
    pub amount: u32,
    pub cause: String,
}
/// Pure bounded decision, run in lexical registration order. Returning zero cancels.
pub trait DamagePolicy: Send + Sync + 'static {
    fn decide(&self, damage: &Damage) -> Result<u32, String>;
}
#[derive(Clone)]
pub struct DamageRegistration {
    pub key: String,
    pub revision: u64,
    pub policy: Arc<dyn DamagePolicy>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Died,
    Respawned,
}
impl EventKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Died => "died",
            Self::Respawned => "respawned",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub player: Player,
    pub before: View,
    pub health: View,
    pub cause: Option<String>,
}
/// Runs in the initiating transaction. Effects and the health transition commit together.
pub trait Hook: Send + Sync + 'static {
    fn handle(&self, context: &mut Context<'_>, event: &Event) -> Result<(), Error>;
}
#[derive(Clone)]
pub struct HookRegistration {
    pub key: String,
    pub revision: u64,
    pub hook: Arc<dyn Hook>,
}
macro_rules! registration {
    ($ty:ty) => {
        impl $ty {
            pub fn validate(&self) -> Result<(), RegistrationError> {
                if !crate::regions::valid_key(&self.key) || self.revision == 0 {
                    return Err(RegistrationError("invalid health registration".into()));
                }
                Ok(())
            }
        }
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("key", &self.key)
                    .field("revision", &self.revision)
                    .finish_non_exhaustive()
            }
        }
    };
}
registration!(DamageRegistration);
registration!(HookRegistration);
#[cfg(test)]
mod tests;
