//! Player lifecycle decisions. Saved progress belongs to exact profiles;
//! temporary participation belongs to one server-issued session.
use crate::{
    RegistrationError,
    gameplay::{Context, Error, Player},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Joining,
    Joined,
    Leaving,
    Left,
    Spawned,
    ProfileTick,
    SessionTick,
}
impl EventKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Joining => "PlayerJoining",
            Self::Joined => "PlayerJoined",
            Self::Leaving => "PlayerLeaving",
            Self::Left => "PlayerLeft",
            Self::Spawned => "PlayerSpawned",
            Self::ProfileTick => "ProfileTick",
            Self::SessionTick => "SessionTick",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub profile: u128,
    pub player: Option<Player>,
    pub transition: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub data: Vec<u8>,
    pub public_data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Decision {
    /// None retains the existing profile state.
    pub state: Option<State>,
    pub session_data: Option<Vec<u8>>,
    /// None retains the deadline; Some(None) suspends.
    pub profile_delay: Option<Option<u32>>,
    pub session_delay: Option<Option<u32>>,
    /// Joining only; host validates before installing the avatar.
    pub spawn: Option<[f32; 3]>,
    pub deny: Option<String>,
}

pub trait Behavior: Send + Sync + 'static {
    fn handle(
        &self,
        context: &mut Context<'_>,
        event: &Event,
        state: &State,
        session_data: &[u8],
    ) -> Result<Decision, Error>;
}

#[derive(Clone)]
pub struct Registration {
    pub key: String,
    pub version: u64,
    pub max_state_bytes: u16,
    pub initial_state: Vec<u8>,
    pub behavior: Arc<dyn Behavior>,
}
impl Registration {
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let valid = self.key.split_once(':').is_some_and(|(namespace, local)| {
            !namespace.is_empty()
                && !local.is_empty()
                && self.key.len() <= 128
                && namespace
                    .bytes()
                    .chain(local.bytes())
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
        });
        if !valid
            || self.version == 0
            || !(1..=4096).contains(&self.max_state_bytes)
            || self.initial_state.len() > self.max_state_bytes as usize
        {
            return Err(RegistrationError(
                "invalid player lifecycle declaration".into(),
            ));
        }
        Ok(())
    }
    pub fn fingerprint_bytes(&self) -> Vec<u8> {
        let mut bytes = self.version.to_le_bytes().to_vec();
        bytes.extend(self.max_state_bytes.to_le_bytes());
        bytes.extend(&self.initial_state);
        bytes
    }
}

impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration")
            .field("key", &self.key)
            .field("version", &self.version)
            .field("max_state_bytes", &self.max_state_bytes)
            .finish_non_exhaustive()
    }
}
