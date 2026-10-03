//! Profile effects use the existing owner WAL; sessions remain exact and transient.
use crate::server::{
    State,
    durable::{CommitAction, StageError},
    parallel::{OwnerData, OwnerKey},
    registry::SystemId,
    runtime::{
        owner_codec::{OwnerCodecError, OwnerValueCodec},
        owner_durable::OwnerSystemConfig,
        systems::SystemRuntime,
    },
    simulation::TickId,
};
use bloxgloom_host_api::{
    player_modifiers::{self, Effect, Movement, Set},
    players::{Registration, State as ProfileState},
};
use std::{collections::BTreeMap, io, sync::Arc};

pub(in crate::server) fn system() -> SystemId {
    SystemId::new(player_modifiers::PROFILE_SYSTEM).expect("builtin modifier system")
}
struct NoCallbacks;
impl bloxgloom_host_api::players::Behavior for NoCallbacks {
    fn handle(
        &self,
        _: &mut bloxgloom_host_api::gameplay::Context<'_>,
        _: &bloxgloom_host_api::players::Event,
        _: &ProfileState,
        _: &[u8],
    ) -> Result<bloxgloom_host_api::players::Decision, bloxgloom_host_api::gameplay::Error> {
        Ok(Default::default())
    }
}
/// Internal profile codec descriptor, never exposed as a package lifecycle.
pub(in crate::server) fn registration() -> Registration {
    Registration {
        key: player_modifiers::PROFILE_SYSTEM.into(),
        version: 1,
        max_state_bytes: player_modifiers::MAX_STATE_BYTES as u16,
        initial_state: Set::default().encode().expect("empty modifier set"),
        behavior: Arc::new(NoCallbacks),
    }
}
struct Codec(Arc<dyn OwnerValueCodec>);
impl OwnerValueCodec for Codec {
    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        validate(value)?;
        self.0.encode(value)
    }
    fn decode(&self, bytes: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        let value = self.0.decode(bytes)?;
        validate(&value)?;
        Ok(value)
    }
}
fn validate(value: &OwnerData) -> Result<(), OwnerCodecError> {
    let value = value
        .get::<ProfileState>()
        .ok_or(OwnerCodecError::InvalidData)?;
    if !value.public_data.is_empty() || Set::decode(&value.data).is_err() {
        return Err(OwnerCodecError::InvalidData);
    }
    Ok(())
}
pub(in crate::server) fn config() -> io::Result<OwnerSystemConfig> {
    let mut config = super::state::config(&registration())?;
    config.codec = Arc::new(Codec(config.codec));
    config.max_bytes = player_modifiers::MAX_STATE_BYTES + 4;
    Ok(config)
}
pub(in crate::server) fn capture_profile(
    runtime: &SystemRuntime,
    profile: u128,
) -> io::Result<bloxgloom_host_api::gameplay::ProfileCell> {
    let system = system();
    let (revision, initialized, state) =
        match runtime.owner_snapshot(&system, OwnerKey::Profile(profile)) {
            Some((revision, value)) => (
                revision,
                true,
                value
                    .get::<ProfileState>()
                    .cloned()
                    .ok_or_else(|| io::Error::other("invalid modifier owner cell"))?,
            ),
            None => (
                0,
                false,
                ProfileState {
                    data: registration().initial_state,
                    public_data: vec![],
                },
            ),
        };
    Set::decode(&state.data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(bloxgloom_host_api::gameplay::ProfileCell {
        revision,
        initialized,
        state,
        next_tick: runtime.profile_deadline(&system, profile),
    })
}
#[derive(Default)]
pub(in crate::server) struct Runtime {
    sessions: BTreeMap<(u128, u64), Set>,
}
impl Runtime {
    pub(in crate::server) fn capture(&self, profile: u128, session: u64) -> Set {
        self.sessions
            .get(&(profile, session))
            .cloned()
            .unwrap_or_default()
    }
    pub(in crate::server) fn apply(
        &mut self,
        profile: u128,
        session: u64,
        key: &str,
        value: Option<Effect>,
        tick: u64,
    ) -> io::Result<()> {
        if profile == 0 || session == 0 {
            return Err(io::Error::other("invalid modifier session"));
        }
        let target = (profile, session);
        let effects = self.sessions.entry(target).or_default();
        effects.retain_active(tick);
        if let Some(value) = value {
            if value.key != key {
                return Err(io::Error::other("modifier key mismatch"));
            }
            effects
                .set(value, tick)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        } else {
            effects.remove(key);
        }
        if effects.iter().next().is_none() {
            self.sessions.remove(&target);
        }
        Ok(())
    }
    pub(in crate::server) fn leaving(&mut self, profile: u128, session: u64) {
        self.sessions.remove(&(profile, session));
    }
    fn expire(&mut self, tick: u64) {
        self.sessions.retain(|_, effects| {
            effects.retain_active(tick);
            effects.iter().next().is_some()
        });
    }
}
pub(in crate::server) fn effective(
    runtime: &Runtime,
    systems: &SystemRuntime,
    profile: u128,
    session: u64,
    tick: u64,
) -> io::Result<Movement> {
    let cell = capture_profile(systems, profile)?;
    let profile_set =
        Set::decode(&cell.state.data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let empty = Set::default();
    Ok(player_modifiers::aggregate(
        &profile_set,
        runtime.sessions.get(&(profile, session)).unwrap_or(&empty),
        tick,
    ))
}

/// At most four durable expiry jobs per tick use the existing sparse owner due
/// index, including disconnected profiles. Inactive effects stop immediately;
/// WAL cleanup makes that expiry survive an otherwise idle restart.
pub(in crate::server) fn drive(state: &mut State, tick: TickId) -> io::Result<()> {
    state.player_modifiers.expire(tick.get());
    let system = system();
    for (_, profile) in state.system_runtime.due_profiles(&system, tick.get(), 4) {
        let cell = capture_profile(&state.system_runtime, profile)?;
        let mut effects = Set::decode(&cell.state.data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        effects.retain_active(tick.get());
        let next = effects.iter().filter_map(|effect| effect.expires_at).min();
        let changes = super::state::prepare(
            &state.system_runtime,
            &registration(),
            profile,
            ProfileState {
                data: effects
                    .encode()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?,
                public_data: vec![],
            },
            next,
        )?;
        let mut reads = crate::server::durable::TerrainReads::default();
        reads.profile(&system, profile, cell.initialized.then_some(cell.revision))?;
        let action = CommitAction {
            client_id: None,
            profile: Some(profile),
            action_id: None,
            receipt_value: None,
            receipt_transition: None,
            inventory_before: None,
            inventory: None,
            world_edits: vec![],
            terrain_reads: reads,
            deltas: vec![],
            changed_cells: vec![],
            pickups: vec![],
            fire_seed: None,
            clock_change: None,
            weather_change: None,
            entities: None,
            entity_wakes: vec![],
            owner_changes: changes,
            sounds: vec![],
            player_publication: None,
        };
        match state.durability.try_stage(tick, &action, None) {
            Ok(_) | Err(StageError::Conflict | StageError::Full) => {}
            Err(error) => {
                return Err(io::Error::other(format!(
                    "modifier expiry stage failed: {error:?}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "modifiers/tests.rs"]
mod tests;
