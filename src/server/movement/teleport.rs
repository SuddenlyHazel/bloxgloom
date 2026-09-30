//! Ordered movement reset: input from the old location cannot move the new one.
use super::MovementState;
use crate::server::{ServerMessage, State};
use crate::world::world_to_chunk;
use std::io;

#[derive(Default)]
pub(in crate::server) struct Reset {
    generation: u64,
    pub(in crate::server) pending: bool,
}
pub(in crate::server) fn teleport(
    state: &mut State,
    id: u64,
    position: [f32; 3],
) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    let generation = client
        .movement_reset
        .generation
        .checked_add(1)
        .ok_or_else(|| io::Error::other("movement reset identity exhausted"))?;
    let profile = client.profile;
    let session = client.action_epoch;
    let floor = client
        .pending_moves
        .back()
        .map_or(client.movement.last_seq(), |m| {
            m.seq.max(client.movement.last_seq())
        });
    if let Err(error) = state.position_store.save(profile, position) {
        state.durability.failed = true;
        return Err(error);
    }
    let delta = state
        .player_entities
        .update_position(id, position)
        .map_err(io::Error::other)?;
    let client = state.clients.get_mut(&id).unwrap();
    client.pending_moves.clear();
    client.movement = MovementState::new(position, floor);
    client.center = world_to_chunk(
        position[0].floor() as i32,
        position[1].floor() as i32,
        position[2].floor() as i32,
    )
    .0;
    client.movement_reset = Reset {
        generation,
        pending: true,
    };
    let sent = client.enqueue(ServerMessage::PlayerTeleport {
        profile,
        session,
        reset: generation,
        position,
    });
    if let Some(delta) = delta {
        state.queue_player_entity_deltas(vec![delta])?;
    }
    // Publish motion before removal so slow-peer cleanup cannot leave a later
    // upsert resurrecting the avatar in other peers' replicas.
    if !sent {
        state.remove_client(id);
    }
    Ok(())
}

pub(in crate::server) fn ready(
    state: &mut State,
    id: u64,
    session: u64,
    reset: u64,
    next_seq: u64,
) -> io::Result<()> {
    let Some(client) = state.clients.get_mut(&id) else {
        return Ok(());
    };
    if session != client.action_epoch
        || reset != client.movement_reset.generation
        || !client.movement_reset.pending
    {
        return Ok(());
    };
    if next_seq == 0 || next_seq == u64::MAX || next_seq <= client.movement.last_seq() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid movement reset sequence",
        ));
    }
    client.movement = MovementState::new(client.position(), next_seq - 1);
    client.movement_reset.pending = false;
    Ok(())
}
