//! Snapshot only committed public projections; queue pressure retries next tick.
use super::State;
use crate::protocol::{PlayerState, ServerMessage};
use crate::server::{parallel::OwnerKey, registry::SystemId};
use bloxgloom_host_api::players::State as ProfileState;
use std::io;

pub(in crate::server) fn publish(state: &mut State) -> io::Result<()> {
    if !state.world.catalog().player_delivery_enabled() {
        return Ok(());
    }
    if state
        .clients
        .values()
        .all(|c| c.last_player_state_revision != 0)
    {
        return Ok(());
    }
    let services = state
        .world
        .catalog()
        .player_lifecycles()
        .map(|r| {
            SystemId::new(&r.key)
                .map(|id| (r.key.clone(), id))
                .map_err(|_| io::Error::other("invalid player service"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    for client in state.clients.values_mut() {
        if client.last_player_state_revision != 0
            || !state
                .player_runtime
                .is_admitted(client.profile, client.action_epoch)
        {
            continue;
        }
        let mut states = Vec::with_capacity(services.len());
        for (key, system) in &services {
            let (revision, public) = match state
                .system_runtime
                .owner_snapshot(system, OwnerKey::Profile(client.profile))
            {
                Some((revision, value)) => (
                    revision,
                    value
                        .get::<ProfileState>()
                        .ok_or_else(|| io::Error::other("invalid player state projection"))?
                        .public_data
                        .clone(),
                ),
                None => (0, vec![]),
            };
            states.push(PlayerState {
                key: key.clone(),
                revision,
                public,
            });
        }
        states.sort_by(|a, b| a.key.cmp(&b.key));
        if client.last_player_states.as_ref() == Some(&states)
            || client
                .sender
                .try_send(ServerMessage::PlayerStates {
                    profile: client.profile,
                    session: client.action_epoch,
                    snapshot: state.player_state_revision,
                    states: states.clone(),
                })
                .is_ok()
        {
            client.last_player_state_revision = state.player_state_revision;
            client.last_player_states = Some(states);
        }
    }
    Ok(())
}

/// Publish only after the durable owner cells are installed. Private data stays
/// server-side; delivery projects registered public bytes to that profile alone.
pub(in crate::server) fn publish_changes(
    state: &mut crate::server::State,
    changes: &[crate::server::journal::Change],
) -> io::Result<()> {
    let affected: std::collections::BTreeSet<_> = changes
        .iter()
        .filter_map(|change| {
            crate::server::runtime::owner_codec::decode_owner_state_key(&change.key)
        })
        .filter_map(|(key, owner)| match owner {
            OwnerKey::Profile(profile)
                if state
                    .world
                    .catalog()
                    .player_lifecycles()
                    .any(|reg| reg.key == key) =>
            {
                Some((key, profile))
            }
            _ => None,
        })
        .collect();
    if affected.is_empty() {
        return Ok(());
    }
    state.player_state_revision = state
        .player_state_revision
        .checked_add(1)
        .ok_or_else(|| io::Error::other("player state revision exhausted"))?;
    for client in state.clients.values_mut() {
        if affected
            .iter()
            .any(|(_, profile)| *profile == client.profile)
        {
            client.last_player_state_revision = 0;
        }
    }
    state
        .player_runtime
        .failed
        .retain(|(key, profile, _, _)| !affected.contains(&(key.clone(), *profile)));
    Ok(())
}
