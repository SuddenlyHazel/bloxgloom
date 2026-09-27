//! Two fixed admission lanes; no wall-clock priorities or speculative apply.
use super::*;

pub(super) const LANE_CAPACITY: usize = MAX_DURABLE_LANE_ACTIONS;
pub(super) const PICKUP_CAPACITY: usize = LANE_CAPACITY / 4;
pub(super) const COMMAND_CAPACITY: usize = LANE_CAPACITY - PICKUP_CAPACITY;
pub(super) const PER_CLIENT_COMMAND_CAPACITY: usize = 8;

pub(super) fn command_room(state: &State, id: u64) -> bool {
    let mut commands = 0;
    let mut own = 0;
    for request in &state.durability.queued {
        if let DurableRequest::Command { id: actor, .. } = request {
            commands += 1;
            own += usize::from(*actor == id);
        }
    }
    commands < COMMAND_CAPACITY && own < PER_CLIENT_COMMAND_CAPACITY
}

pub(super) fn player_request(request: &DurableRequest) -> bool {
    matches!(
        request,
        DurableRequest::Command { .. } | DurableRequest::Pickup { .. }
    )
}

pub(super) fn run(state: &mut State, tick: TickId) -> io::Result<()> {
    let (mut players, mut simulation): (std::collections::VecDeque<_>, _) =
        std::mem::take(&mut state.durability.queued)
            .into_iter()
            .partition(player_request);
    let has_simulation = !simulation.is_empty();
    let contested = !players.is_empty() && has_simulation;
    // Count actual mixed admission rounds, not tick parity: intermittent work
    // cannot repeatedly miss the simulation-first opportunity.
    let simulation_first = contested && state.durability.response_turn == 3;
    let mut blocked_profiles = HashSet::new();
    let mut player_staged = false;
    let mut motion_staged = false;
    for player_lane in [!simulation_first, simulation_first] {
        let lane = if player_lane {
            &mut players
        } else {
            &mut simulation
        };
        let tail = lane.split_off(lane.len().min(LANE_CAPACITY));
        state.durability.queued = std::mem::take(lane);
        let mut deferred = std::collections::VecDeque::new();
        let admission_start = state.durability.next_id;
        let mut plans = if player_lane {
            std::collections::VecDeque::new()
        } else {
            let (staged, plans) = stage_motion_batch(state, tick, &mut deferred)?;
            motion_staged |= staged;
            plans
        };
        process_queue(state, tick, &mut plans, &mut blocked_profiles)?;
        if player_lane {
            player_staged |= state.durability.next_id != admission_start;
        }
        debug_assert!(plans.is_empty());
        *lane = std::mem::take(&mut state.durability.queued);
        lane.extend(deferred);
        lane.extend(tail);
    }
    players.append(&mut simulation);
    state.durability.queued = players;
    if motion_staged || (has_simulation && player_staged) {
        // One existing simulation boundary, after both lanes. In particular a
        // conflicting command and entity cannot leave reservations stretching
        // into successive turns and phase-lock either lane out of admission.
        // No apply occurs between planning the two lanes; both use committed
        // tick-start state, and every accepted preimage remains fenced.
        drain_staged_receipts(state)?;
    }
    if contested {
        state.durability.response_turn = (state.durability.response_turn + 1) % 4;
    }
    Ok(())
}
