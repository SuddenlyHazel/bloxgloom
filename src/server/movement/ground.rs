//! Server-owned walking gravity and jump impulses, including idle players.
use super::*;

const DT: f32 = 0.02;
const GRAVITY: f32 = 20.0;
const TERMINAL_SPEED: f32 = 24.0;
const JUMP_SPEED: f32 = 8.0;

pub(in crate::server) fn set_flying(state: &mut crate::server::State, id: u64, flying: bool) {
    if flying
        && state
            .clients
            .get(&id)
            .is_some_and(|c| state.admin_profile == Some(c.profile))
    {
        super::sprint::stop(state, id);
    }
    if let Some(client) = state.clients.get_mut(&id) {
        if state.admin_profile == Some(client.profile) && client.movement.flying != flying {
            client.movement.flying = flying;
            client.movement.vertical_velocity = 0.0;
            client.movement.jump_requested = false;
        }
        // A rejected request still reports the authoritative mode, allowing
        // the client to leave its pending state without optimistic authority.
        client.enqueue(crate::protocol::ServerMessage::FlyingMode {
            flying: client.movement.flying,
        });
    }
}

pub(super) fn advance(view: &VoxelView, state: &mut MovementState) -> Result<(), MissingChunk> {
    let mut next = *state;
    let mut body = view.player_rules().for_stance(state.crouching).body();
    body.foot_inset = 0.0;
    let clear = |p| {
        body.collides(p, |x, y, z| view.is_solid(x, y, z))
            .map(|hit| !hit)
    };
    // Flight permits the catalog's small foot inset. Turning it off while
    // touching a floor must lift that inset onto the voxel top, not trap the
    // player inside the stricter walking body. Never escape a new solid block.
    if !clear(next.position)? {
        let original = view.player_rules().for_stance(state.crouching).body();
        let mut lifted = next.position;
        lifted[1] = lifted[1].ceil();
        if lifted[1] - next.position[1] <= original.foot_inset + 0.0001
            && !original.collides(next.position, |x, y, z| view.is_solid(x, y, z))?
            && clear(lifted)?
        {
            next.position = lifted;
        }
    }
    if !clear(next.position)? {
        // Edits that embed a player must not let gravity tunnel them out.
        next.vertical_velocity = 0.0;
        next.jump_requested = false;
        *state = next;
        return Ok(());
    }
    let mut below = next.position;
    below[1] -= 0.002;
    let grounded = !clear(below)?;
    if grounded && next.vertical_velocity <= 0.0 {
        next.vertical_velocity = if next.jump_requested {
            JUMP_SPEED * next.modifiers.jump
        } else {
            0.0
        };
    }
    next.jump_requested = false;
    if !grounded || next.vertical_velocity > 0.0 {
        next.vertical_velocity =
            (next.vertical_velocity - GRAVITY * next.modifiers.gravity * DT).max(-TERMINAL_SPEED);
        let distance = next.vertical_velocity * DT;
        let steps = (distance.abs() / 0.1).ceil().max(1.0) as usize;
        for _ in 0..steps {
            let mut candidate = next.position;
            candidate[1] += distance / steps as f32;
            if !clear(candidate)? {
                if distance < 0.0 {
                    candidate[1] = candidate[1].ceil();
                    if clear(candidate)? {
                        next.position = candidate;
                    }
                }
                next.vertical_velocity = 0.0;
                break;
            }
            next.position = candidate;
        }
    }
    // Unknown terrain aborts the whole vertical step, including its velocity.
    *state = next;
    Ok(())
}
