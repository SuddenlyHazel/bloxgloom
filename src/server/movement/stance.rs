//! Ephemeral authoritative posture, independent of saved appearance/profile data.
use super::*;

pub(super) fn resolve(view: &VoxelView, state: &mut MovementState) -> Result<(), MissingChunk> {
    if !state.stance_pending() {
        return Ok(());
    }
    if !state.requested_crouch
        && view
            .player_rules()
            .body()
            .collides(state.position, |x, y, z| view.is_solid(x, y, z))?
    {
        return Ok(());
    }
    state.crouching = state.requested_crouch;
    // Standing credit must never be spendable at the crouched rate.
    state.credit_nanoblocks = 0;
    Ok(())
}

pub(super) fn broadcast(state: &mut crate::server::State, changes: &[(u64, bool)]) {
    let mut disconnected = Vec::new();
    for (&id, client) in &state.clients {
        for &(session, crouching) in changes {
            if crouching && !state.clients.contains_key(&session) {
                continue;
            }
            let entity_id = crate::server::entities::EntityId::for_player_session(session)
                .expect("admitted session entity")
                .get();
            if !client.enqueue(crate::protocol::ServerMessage::PlayerStance {
                entity_id,
                crouching,
            }) {
                disconnected.push(id);
                break;
            }
        }
    }
    for id in disconnected {
        state.remove_client(id);
    }
}

/// Clear remote posture before a removed session can leave a stale map entry.
pub(in crate::server) fn clear(state: &mut crate::server::State, session: u64) {
    broadcast(state, &[(session, false)]);
}
