//! Server player services, separate from world/entity ownership.
use super::State;
pub(super) mod admission;
mod callbacks;
pub(super) mod chat;
pub(super) mod delivery;
pub(super) mod inventory;
mod lifecycle;
pub(super) mod modifiers;
mod operations;
mod pending;
mod preparation;
pub(super) mod regions;
pub(super) use callbacks::admit;
pub(super) use pending::JoinGuard;
pub(super) mod state;
use bloxgloom_host_api::gameplay::Player;
pub(super) use lifecycle::{Published, Runtime, committed, drive, joined, leaving};

pub(super) fn capture(state: &State) -> Vec<Player> {
    let mut players: Vec<_> = state
        .clients
        .iter()
        .map(|(&id, client)| {
            let packaged = state
                .player_entities
                .appearance_state_for_session(id)
                .and_then(|a| a.packaged);
            Player {
                profile: client.profile,
                session: client.action_epoch,
                entity: state
                    .player_entities
                    .id_for_session(id)
                    .map_or(0, |id| id.get()),
                name: client.name.clone(),
                position: client.position(),
                appearance: state
                    .player_entities
                    .appearance_for_session(id)
                    .unwrap_or([0; 4]),
                model: packaged
                    .and_then(|p| state.world.catalog().model_key(p.model).map(str::to_owned)),
                model_visual: packaged.map(|p| p.visual),
            }
        })
        .collect();
    players.sort_by_key(|player| player.profile);
    players
}

/// A full bounded snapshot retries under queue pressure, so membership converges.
pub(super) fn publish_roster(state: &mut State) {
    let mut players = state
        .clients
        .values()
        .filter(|c| state.player_runtime.is_admitted(c.profile, c.action_epoch))
        .map(|c| crate::protocol::PlayerSummary {
            profile: c.profile,
            session: c.action_epoch,
            name: c.name.clone(),
        })
        .collect::<Vec<_>>();
    players.sort_by_key(|p| p.profile);
    for client in state.clients.values_mut() {
        if client.last_roster_revision != state.roster_revision
            && client
                .sender
                .try_send(crate::protocol::ServerMessage::PlayerRoster {
                    revision: state.roster_revision,
                    players: players.clone(),
                })
                .is_ok()
        {
            client.last_roster_revision = state.roster_revision;
        }
    }
}

impl Runtime {
    pub(in crate::server) fn current_tick(&self) -> u64 {
        self.tick
    }
}
