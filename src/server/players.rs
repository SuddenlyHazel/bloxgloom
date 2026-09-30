//! Server player services, separate from world/entity ownership.
use super::State;
pub(super) mod admission;
use bloxgloom_host_api::gameplay::Player;

pub(super) fn capture(state: &State) -> Vec<Player> {
    let mut players: Vec<_> = state
        .clients
        .iter()
        .map(|(&id, client)| Player {
            profile: client.profile,
            session: client.action_epoch,
            entity: state
                .player_entities
                .id_for_session(id)
                .map_or(0, |id| id.get()),
            name: client.name.clone(),
            position: client.position(),
        })
        .collect();
    players.sort_by_key(|player| player.profile);
    players
}
