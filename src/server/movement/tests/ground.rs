use super::*;

fn floor(roof: bool) -> VoxelView {
    let mut chunk = air_chunk(key(0, 0, 0));
    for x in 0..16 {
        for z in 0..16 {
            chunk
                .blocks
                .set(Chunk::index([x, 0, z]).unwrap(), crate::world::STONE);
            if roof {
                chunk
                    .blocks
                    .set(Chunk::index([x, 3, z]).unwrap(), crate::world::STONE);
            }
        }
    }
    VoxelView::from_chunks([chunk]).unwrap()
}
fn walking(y: f32) -> MovementState {
    let mut state = MovementState::new([1.5, y, 1.5], 0);
    state.flying = false;
    state
}

#[test]
fn sprint_rate_is_authoritative_bounded_and_cannot_bank_faster_credit_after_stopping() {
    let terrain = floor(false);
    let walk = process_movement_batch(&terrain, walking(1.0), &[command(1, [0.24, 0.0, 0.0])]);
    assert_eq!(walk.consumed, 0);
    let mut state = walking(1.0);
    assert!(state.request_sprint(true));
    let sprint = process_movement_batch(&terrain, state, &[command(1, [0.24, 0.0, 0.0])]);
    assert_eq!(sprint.consumed, 1);
    assert_eq!(sprint.state.position()[1], 1.0);
    close(sprint.state.position()[0], 1.74);
    state = sprint.state;
    for _ in 0..20 {
        state.advance_idle_tick(terrain.player_rules());
    }
    let credit = state.credit_nanoblocks();
    assert!(!state.request_sprint(true));
    assert_eq!(
        state.credit_nanoblocks(),
        credit,
        "duplicate requests cannot refill credit"
    );
    assert!(state.request_sprint(false));
    assert_eq!(state.credit_nanoblocks(), 0);
    assert_eq!(
        process_movement_batch(&terrain, state, &[command(2, [0.24, 0.0, 0.0])]).consumed,
        0
    );
    state.request_crouch(true);
    assert!(!state.request_sprint(true));
    state = MovementState::new([1.5, 1.0, 1.5], 0);
    assert!(!state.request_sprint(true), "flight cannot sprint");
    let mut state = walking(1.0);
    state.request_sprint(true);
    let excessive = process_movement_batch(&terrain, state, &[command(1, [4.0, 0.0, 0.0])]);
    assert_eq!(excessive.acknowledgments[0].kind, AckKind::Rejected);
    assert_eq!(excessive.state.position(), [1.5, 1.0, 1.5]);
}

#[test]
fn walking_idle_gravity_accelerates_and_lands_exactly_without_command_credit() {
    let terrain = floor(false);
    let mut state = walking(6.0);
    let first = process_movement_batch(&terrain, state, &[]);
    close(first.state.position()[1], 5.992);
    state = first.state;
    let second = process_movement_batch(&terrain, state, &[]);
    assert!(state.position()[1] - second.state.position()[1] > 6.0 - state.position()[1]);
    state = second.state;
    for _ in 0..48 {
        state = process_movement_batch(&terrain, state, &[]).state;
    }
    assert_eq!(state.position(), [1.5, 1.0, 1.5]);
    assert_eq!(state.vertical_velocity, 0.0);
    let held = process_movement_batch(&terrain, state, &[]);
    assert_eq!(held.state.position(), state.position());
    let commands: Vec<_> = (1..=MAX_COMMANDS_PER_TICK as u64)
        .map(|seq| command(seq, [0.0; 3]))
        .collect();
    let idle = process_movement_batch(&terrain, walking(6.0), &[]);
    let flood = process_movement_batch(&terrain, walking(6.0), &commands);
    assert_eq!(
        flood.state.position(),
        idle.state.position(),
        "input count must not advance gravity"
    );
}

#[test]
fn walking_ignores_client_vertical_displacement_and_jumps_only_from_support() {
    let terrain = floor(false);
    let raised = process_movement_batch(&terrain, walking(1.0), &[command(1, [0.0, 0.1, 0.0])]);
    assert_eq!(raised.state.position()[1], 1.0);
    let mut state = raised.state;
    state.request_jump();
    state = process_movement_batch(&terrain, state, &[]).state;
    assert!(state.position()[1] > 1.0);
    let velocity = state.vertical_velocity;
    state.request_jump();
    state = process_movement_batch(&terrain, state, &[]).state;
    assert!(
        state.vertical_velocity < velocity,
        "a second jump cannot lift an airborne player"
    );
    for _ in 0..100 {
        state = process_movement_batch(&terrain, state, &[]).state;
    }
    assert_eq!(state.position()[1], 1.0);
}

#[test]
fn walking_jump_hits_ceiling_and_unknown_terrain_keeps_velocity_and_position() {
    let roof = floor(true);
    let mut state = walking(1.0);
    state.request_jump();
    for _ in 0..100 {
        state = process_movement_batch(&roof, state, &[]).state;
        assert!(state.position()[1] + roof.player_rules().body().head_height < 3.0);
    }
    assert_eq!(state.position()[1], 1.0);
    let unknown = VoxelView::from_chunks(Vec::<Chunk>::new()).unwrap();
    let mut airborne = walking(6.0);
    airborne.vertical_velocity = -12.0;
    let deferred = process_movement_batch(&unknown, airborne, &[]);
    assert!(matches!(deferred.stop_reason, StopReason::MissingChunk(_)));
    assert_eq!(deferred.state.position(), airborne.position());
    assert_eq!(deferred.state.vertical_velocity, airborne.vertical_velocity);
}

#[test]
fn walking_gravity_continues_when_horizontal_budget_is_exhausted() {
    let terrain = floor(false);
    let batch = process_movement_batch(&terrain, walking(6.0), &[command(1, [1.0, 0.0, 0.0])]);
    assert_eq!(batch.stop_reason, StopReason::MovementBudget);
    assert_eq!(batch.consumed, 0);
    assert!(batch.state.position()[1] < 6.0);
}

#[test]
fn switching_from_flight_floor_inset_does_not_trap_walking_player() {
    let terrain = floor(false);
    let standing = process_movement_batch(&terrain, walking(0.97), &[]);
    assert_eq!(standing.state.position()[1], 1.0);
    let moving = process_movement_batch(&terrain, standing.state, &[command(1, [0.1, 0.0, 0.0])]);
    assert!(moving.state.position()[0] > standing.state.position()[0]);
    let embedded = process_movement_batch(&terrain, walking(0.5), &[]);
    assert_eq!(
        embedded.state.position()[1],
        0.5,
        "edits cannot tunnel a player out of a block"
    );
}
