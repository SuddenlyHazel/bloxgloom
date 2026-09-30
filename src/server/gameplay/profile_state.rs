//! Lazy profile captures keep private values scoped to their owning package.
use super::*;
use crate::server::{parallel::OwnerKey, registry::SystemId};
use bloxgloom_host_api::{gameplay::ProfileCell, players::State};
impl WorldSnapshot<'_> {
    fn profile_registration(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<&bloxgloom_host_api::players::Registration, Error> {
        if key.len() > 128
            || key.split_once(':').map(|v| v.0) != Some(namespace)
            || !self.world.catalog().player_authority(namespace)
        {
            return Err(Error::Invalid(
                "profile state ownership or authority denied".into(),
            ));
        }
        self.world
            .catalog()
            .player_lifecycles()
            .find(|r| r.key == key)
            .map(|r| r.as_ref())
            .ok_or_else(|| Error::Invalid("unregistered player service".into()))
    }
}
impl WorldSnapshot<'_> {
    pub(super) fn capture_profile(
        &mut self,
        namespace: &str,
        key: &str,
        profile: u128,
    ) -> Result<ProfileCell, Error> {
        if profile == 0 {
            return Err(Error::Invalid("invalid profile ID".into()));
        }
        let initial = self
            .profile_registration(namespace, key)?
            .initial_state
            .clone();
        let runtime = self
            .profile_services
            .ok_or_else(|| Error::Invalid("profile state unavailable in this context".into()))?;
        let system =
            SystemId::new(key).map_err(|_| Error::Invalid("invalid player service".into()))?;
        let (revision, initialized, state) =
            match runtime.owner_snapshot(&system, OwnerKey::Profile(profile)) {
                Some((revision, value)) => (
                    revision,
                    true,
                    value
                        .get::<State>()
                        .cloned()
                        .ok_or_else(|| Error::Host("invalid profile cell".into()))?,
                ),
                None => (
                    0,
                    false,
                    State {
                        data: initial,
                        public_data: vec![],
                    },
                ),
            };
        self.reads
            .profile(&system, profile, initialized.then_some(revision))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(ProfileCell {
            revision,
            initialized,
            state,
            next_tick: runtime.profile_deadline(&system, profile),
        })
    }
    pub(super) fn validate_profile(
        &self,
        namespace: &str,
        key: &str,
        state: &State,
    ) -> Result<(), Error> {
        let registration = self.profile_registration(namespace, key)?;
        if !self.player_operations_enabled || self.profile_services.is_none() {
            return Err(Error::Invalid(
                "profile state writes unavailable in this context".into(),
            ));
        }
        if state.data.len() > usize::from(registration.max_state_bytes)
            || state.public_data.len() > 1024
        {
            return Err(Error::Invalid("profile state byte limit exceeded".into()));
        }
        Ok(())
    }
}
