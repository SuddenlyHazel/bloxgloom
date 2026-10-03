//! Durable health uses the existing owner WAL and no parallel mutable owner.
use crate::server::{
    parallel::{OwnerData, OwnerKey},
    registry::SystemId,
    runtime::{
        owner_codec::{OwnerCodecError, OwnerValueCodec},
        owner_durable::OwnerSystemConfig,
        systems::SystemRuntime,
    },
};
use bloxgloom_host_api::{
    player_health::{self, State},
    players::{Registration, State as ProfileState},
};
use std::{io, sync::Arc};

pub(in crate::server) fn system() -> SystemId {
    SystemId::new(player_health::PROFILE_SYSTEM).expect("builtin health system")
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
        key: player_health::PROFILE_SYSTEM.into(),
        version: 1,
        max_state_bytes: player_health::STATE_BYTES as u16,
        initial_state: State::default().encode().expect("empty health set"),
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
    if !value.public_data.is_empty() || State::decode(&value.data).is_err() {
        return Err(OwnerCodecError::InvalidData);
    }
    Ok(())
}
pub(in crate::server) fn config() -> io::Result<OwnerSystemConfig> {
    let mut config = super::state::config(&registration())?;
    config.codec = Arc::new(Codec(config.codec));
    config.max_bytes = player_health::STATE_BYTES + 4;
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
                    .ok_or_else(|| io::Error::other("invalid health owner cell"))?,
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
    State::decode(&state.data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(bloxgloom_host_api::gameplay::ProfileCell {
        revision,
        initialized,
        state,
        next_tick: runtime.profile_deadline(&system, profile),
    })
}

pub(in crate::server) fn view(
    runtime: &SystemRuntime,
    profile: u128,
) -> io::Result<player_health::View> {
    let cell = capture_profile(runtime, profile)?;
    let state = State::decode(&cell.state.data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let revision = if cell.initialized {
        cell.revision
            .checked_add(1)
            .ok_or_else(|| io::Error::other("health revision exhausted"))?
    } else {
        0
    };
    Ok(player_health::View::new(state, revision))
}
/// Publication follows the owner WAL receipt. Re-read its final value so several
/// health writes in one plan cannot advertise an intermediate health state.
pub(in crate::server) fn publish(
    state: &mut crate::server::State,
    id: u64,
    operation: player_health::View,
    position: Option<[f32; 3]>,
) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    let profile = client.profile;
    let session = client.action_epoch;
    let before = client.health;
    let health = view(&state.system_runtime, profile)?;
    if operation.life != health.life || health == before {
        return Ok(());
    }
    state.clients.get_mut(&id).unwrap().health = health;
    if !state.clients[&id].enqueue(crate::protocol::ServerMessage::PlayerHealth {
        profile,
        session,
        health,
    }) {
        state.remove_client(id);
        return Ok(());
    }
    if health.life != before.life {
        let position = position.unwrap_or_else(|| state.clients[&id].position());
        if health.alive {
            state
                .position_store
                .save_with_life(profile, position, health.life)?;
        }
        crate::server::movement::teleport(state, id, position)?;
    }
    Ok(())
}
