//! Captured lifecycle inputs and authoritative admission/spawn validation.
use super::{State, capture, lifecycle::QUEUE_LIMIT};
use crate::{
    inventory::Inventory,
    server::{durable::TerrainReads, parallel::OwnerKey, registry::SystemId},
};
use bloxgloom_host_api::{
    gameplay::Player,
    players::{Decision, Event, EventKind, Registration, State as ProfileState},
};
use std::io;
pub(super) fn profile_state(
    state: &State,
    reg: &Registration,
    profile: u128,
) -> io::Result<ProfileState> {
    let system = SystemId::new(&reg.key).map_err(|_| io::Error::other("invalid player service"))?;
    match state
        .system_runtime
        .owner_snapshot(&system, OwnerKey::Profile(profile))
    {
        Some((_, value)) => value
            .get::<ProfileState>()
            .cloned()
            .ok_or_else(|| io::Error::other("invalid player profile cell")),
        None => Ok(ProfileState {
            data: reg.initial_state.clone(),
            public_data: vec![],
        }),
    }
}
pub(super) fn invoke(
    state: &mut State,
    reg: &Registration,
    event: &Event,
    inventory: &Inventory,
    final_session: Option<&[u8]>,
) -> io::Result<(
    Decision,
    Option<Inventory>,
    Vec<bloxgloom_host_api::gameplay::PlayerOperation>,
    TerrainReads,
)> {
    let players = capture(state);
    let profile = profile_state(state, reg, event.profile)?;
    let session = final_session.map(ToOwned::to_owned).unwrap_or_else(|| {
        event
            .player
            .as_ref()
            .and_then(|p| {
                state
                    .player_runtime
                    .sessions
                    .get(&(reg.key.clone(), p.profile, p.session))
            })
            .map(|s| s.data.clone())
            .unwrap_or_default()
    });
    let mut reads = TerrainReads::default();
    let mut requested = Vec::new();
    let result = crate::server::gameplay::invoke_player(
        &mut state.world,
        &mut reads,
        &mut requested,
        crate::server::gameplay::Participants {
            players: &players,
            action_id: None,
            clock: Some(state.world_time.capture()),
            actor: Some((event.profile, inventory)),
            actor_position: event.player.as_ref().map(|p| p.position),
            admin: false,
            entities: &state.entities,
        },
        state.player_runtime.tick,
        state.seed,
        reg,
        event,
        &profile,
        &session,
    );
    for chunk in requested {
        let _ = crate::server::streaming::request_chunk(state, chunk)?;
    }
    let (decision, inventory, operations) = result?;
    Ok((decision, inventory, operations, reads))
}
/// A pending connection may propose admission/spawn, but cannot publish rewards.
pub(in crate::server) fn admit(
    state: &mut State,
    profile: u128,
    name: &str,
    epoch: u64,
    mut position: [f32; 3],
    inventory: &Inventory,
    appearance: [u8; 4],
) -> io::Result<[f32; 3]> {
    let registrations = state
        .world
        .catalog()
        .player_lifecycles()
        .cloned()
        .collect::<Vec<_>>();
    if state.player_runtime.queue.len() + registrations.len() * 4 > QUEUE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "player lifecycle queue full",
        ));
    }
    for reg in registrations {
        let event = Event {
            kind: EventKind::Joining,
            profile,
            player: Some(Player {
                profile,
                session: epoch,
                entity: 0,
                name: name.into(),
                position,
                appearance,
            }),
            transition: epoch,
        };
        let (decision, inventory_after, operations, _) =
            invoke(state, &reg, &event, inventory, None)?;
        if !operations.is_empty()
            || inventory_after.is_some()
            || decision.state.is_some()
            || decision.session_data.is_some()
            || decision.profile_delay.is_some()
            || decision.session_delay.is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PlayerJoining is an admission/spawn decision; publish rewards in PlayerJoined",
            ));
        }
        if let Some(reason) = decision.deny {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason));
        }
        if let Some(spawn) = decision.spawn {
            if spawn.iter().any(|p| !p.is_finite()) || spawn[1] <= crate::world::BEDROCK_Y as f32 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid spawn proposal",
                ));
            }
            match crate::server::spawn::collides_cached(state, spawn)? {
                Some(false) => position = spawn,
                Some(true) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "spawn proposal collides with terrain",
                    ));
                }
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "spawn proposal terrain is loading",
                    ));
                }
            }
        }
    }
    Ok(position)
}
