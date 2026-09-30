//! Package-owned, revisioned durable profile values; identity is not authority.
use super::{Context, Error};
use crate::players::State;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileCell {
    /// Exact owner revision; an initial persisted cell may also have revision zero.
    pub revision: u64,
    pub initialized: bool,
    pub state: State,
    pub next_tick: Option<u64>,
}
impl Context<'_> {
    pub fn profile_state(&mut self, key: &str, profile: u128) -> Result<ProfileCell, Error> {
        self.charge()?;
        let Some(namespace) = self.handler_namespace.as_deref() else {
            return self.fail(Error::Invalid("profile state requires a handler".into()));
        };
        // Recheck ownership even when an earlier handler populated the overlay.
        let cell = match self.snapshot.profile_state(namespace, key, profile) {
            Ok(cell) => cell,
            Err(error) => return self.fail(error),
        };
        Ok(self
            .plan
            .profile_states
            .get(&(key.into(), profile))
            .cloned()
            .unwrap_or(cell))
    }
    pub fn set_profile_state(
        &mut self,
        key: &str,
        profile: u128,
        state: State,
    ) -> Result<(), Error> {
        let mut cell = self.profile_state(key, profile)?;
        let namespace = self
            .handler_namespace
            .as_deref()
            .expect("validated handler");
        if let Err(error) = self.snapshot.validate_profile_state(namespace, key, &state) {
            return self.fail(error);
        }
        let target = (key.to_owned(), profile);
        if !self.plan.profile_states.contains_key(&target) && self.plan.profile_states.len() >= 64 {
            return self.fail(Error::Invalid("profile state write limit exceeded".into()));
        }
        cell.state = state;
        self.plan.profile_states.insert(target, cell);
        Ok(())
    }
}
