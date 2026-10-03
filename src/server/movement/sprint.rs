//! Session-only sprint authority and bounded public locomotion state.
use super::*;
use crate::server::{ServerMessage, State};

impl MovementState {
    pub fn sprinting(self) -> bool {
        self.sprinting
    }

    pub(super) fn request_sprint(&mut self, sprinting: bool) -> bool {
        let sprinting = sprinting && !self.flying && !self.crouching && !self.requested_crouch;
        if self.sprinting == sprinting {
            return false;
        }
        self.sprinting = sprinting;
        // Faster-rate credit must not be spendable after stopping or crouching.
        self.credit_nanoblocks = 0;
        true
    }
}

pub(in crate::server) fn set_sprinting(state: &mut State, id: u64, requested: bool) {
    let Some(client) = state.clients.get_mut(&id) else {
        return;
    };
    let changed = client
        .movement
        .request_sprint(requested && !client.movement_reset.pending);
    let sprinting = client.movement.sprinting();
    if changed {
        broadcast(state, id, sprinting);
    } else {
        client.enqueue(message(id, sprinting));
    }
}

pub(in crate::server) fn stop(state: &mut State, id: u64) {
    if state
        .clients
        .get(&id)
        .is_some_and(|c| c.movement.sprinting())
    {
        set_sprinting(state, id, false);
    }
}

fn message(session: u64, sprinting: bool) -> ServerMessage {
    ServerMessage::PlayerSprint {
        entity_id: crate::server::entities::EntityId::for_player_session(session)
            .expect("admitted session entity")
            .get(),
        sprinting,
    }
}

pub(in crate::server) fn broadcast(state: &mut State, session: u64, sprinting: bool) {
    let mut disconnected = Vec::new();
    for (&id, client) in &state.clients {
        if !client.enqueue(message(session, sprinting)) {
            disconnected.push(id);
        }
    }
    for id in disconnected {
        state.remove_client(id);
    }
}
