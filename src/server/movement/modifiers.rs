//! Runtime rates share the movement reset fence with teleports. Old predicted
//! displacements cannot consume the budget of a newly applied/expired effect.
use super::*;
use crate::server::simulation::TickId;
use crate::server::{ServerMessage, State};
use std::io;

pub(super) fn synchronize(state: &mut State, tick: TickId) -> io::Result<()> {
    let mut disconnected = Vec::new();
    for (&id, client) in &mut state.clients {
        let effective = crate::server::players::modifiers::effective(
            &state.player_modifiers,
            &state.system_runtime,
            client.profile,
            client.action_epoch,
            tick.get(),
        )?;
        if client.movement.modifiers == effective {
            continue;
        }
        let generation = client
            .movement_reset
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("movement reset identity exhausted"))?;
        let floor = client
            .pending_moves
            .back()
            .map_or(client.movement.last_seq, |m| {
                m.seq.max(client.movement.last_seq)
            });
        client.pending_moves.clear();
        client.movement.last_seq = floor;
        client.movement.credit_nanoblocks = 0;
        client.movement.jump_requested = false;
        client.movement.modifiers = effective;
        client.movement_reset = Reset {
            generation,
            pending: true,
        };
        if !client.enqueue(ServerMessage::PlayerModifiers {
            profile: client.profile,
            session: client.action_epoch,
            reset: generation,
            position: client.position(),
            movement: effective,
        }) {
            disconnected.push(id);
        }
    }
    for id in disconnected {
        state.remove_client(id);
    }
    Ok(())
}
