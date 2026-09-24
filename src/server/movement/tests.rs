use super::{
    AckKind, FLOAT_ROUNDING_ALLOWANCE_PER_TICK, MAX_COMMANDS_PER_TICK, MovementCommand,
    MovementState, StopReason, process_movement_batch,
};
use crate::server::voxel_view::{MissingChunk, VoxelView};
use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey};

fn key(x: i32, y: i32, z: i32) -> ChunkKey {
    ChunkKey { x, y, z }
}

fn air_chunk(key: ChunkKey) -> Chunk {
    Chunk {
        key,
        version: 1,
        blocks: vec![AIR; CHUNK_VOLUME],
    }
}

fn view() -> VoxelView {
    VoxelView::from_chunks([air_chunk(key(0, 0, 0))]).unwrap()
}

fn command(seq: u64, delta: [f32; 3]) -> MovementCommand {
    MovementCommand { seq, delta }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

#[test]
fn sixty_hz_commands_batched_in_one_tick_wait_for_fixed_credit() {
    let voxel_view = view();
    let state = MovementState::new([1.0, 1.0, 1.0], 0);
    let first_tick = process_movement_batch(
        &voxel_view,
        state,
        &[
            command(1, [1.0 / 6.0, 0.0, 0.0]),
            command(2, [1.0 / 6.0, 0.0, 0.0]),
        ],
    );

    assert_eq!(first_tick.consumed, 1);
    assert_eq!(first_tick.stop_reason, StopReason::MovementBudget);
    assert_eq!(first_tick.acknowledgments[0].seq, 1);
    close(first_tick.state.position()[0], 1.0 + 1.0 / 6.0);

    let next_tick = process_movement_batch(
        &voxel_view,
        first_tick.state,
        &[command(2, [1.0 / 6.0, 0.0, 0.0])],
    );
    assert_eq!(next_tick.consumed, 1);
    assert_eq!(next_tick.stop_reason, StopReason::InputDrained);
    close(next_tick.state.position()[0], 1.0 + 2.0 / 6.0);
}

#[test]
fn many_commands_cannot_mint_per_command_speed_credit() {
    let voxel_view = view();
    let commands: Vec<_> = (1..=40).map(|seq| command(seq, [0.1, 0.0, 0.0])).collect();
    let batch = process_movement_batch(
        &voxel_view,
        MovementState::new([1.0, 1.0, 1.0], 0),
        &commands,
    );

    assert_eq!(batch.consumed, 2);
    assert_eq!(batch.state.last_seq(), 2);
    assert!(batch.state.credit_nanoblocks() <= FLOAT_ROUNDING_ALLOWANCE_PER_TICK);
    close(batch.state.position()[0], 1.2);
}

#[test]
fn replayed_sequences_are_ignored_without_reapplying_movement() {
    let voxel_view = view();
    let moved = process_movement_batch(
        &voxel_view,
        MovementState::new([1.0, 1.0, 1.0], 0),
        &[command(4, [0.1, 0.0, 0.0])],
    );
    let position = moved.state.position();
    let replay = process_movement_batch(
        &voxel_view,
        moved.state,
        &[command(4, [0.1, 0.0, 0.0]), command(3, [0.1, 0.0, 0.0])],
    );

    assert_eq!(replay.consumed, 2);
    assert!(replay.acknowledgments.is_empty());
    assert_eq!(replay.state.position(), position);
    assert_eq!(replay.state.last_seq(), 4);
}

#[test]
fn invalid_and_excessive_deltas_consume_sequence_without_changing_position() {
    let voxel_view = view();
    let batch = process_movement_batch(
        &voxel_view,
        MovementState::new([1.0, 1.0, 1.0], 0),
        &[
            command(1, [f32::INFINITY, 0.0, 0.0]),
            command(2, [2.6, 0.0, 0.0]),
            command(3, [0.1, 0.0, 0.0]),
        ],
    );

    assert_eq!(batch.consumed, 3);
    assert_eq!(batch.acknowledgments[0].kind, AckKind::Rejected);
    assert_eq!(batch.acknowledgments[1].kind, AckKind::Rejected);
    assert_eq!(batch.acknowledgments[2].kind, AckKind::Resolved);
    assert_eq!(batch.state.last_seq(), 3);
    assert!(batch.state.credit_nanoblocks() > 100_000_000);
    assert!(batch.state.credit_nanoblocks() < 100_000_256);
    close(batch.acknowledgments[0].position[0], 1.0);
    close(batch.acknowledgments[1].position[0], 1.0);
    close(batch.state.position()[0], 1.1);
}

#[test]
fn missing_chunk_defers_that_command_and_the_later_ordered_suffix() {
    let voxel_view = view();
    let mut state = MovementState::new([15.5, 1.0, 1.0], 0);
    // Accumulate the bounded 250 ms burst allowance before approaching the seam.
    for _ in 0..12 {
        state = process_movement_batch(&voxel_view, state, &[]).state;
    }

    let batch = process_movement_batch(
        &voxel_view,
        state,
        &[
            command(1, [0.1, 0.0, 0.0]),
            command(2, [0.2, 0.0, 0.0]),
            command(3, [0.1, 0.0, 0.0]),
        ],
    );

    let missing = MissingChunk { key: key(1, 0, 0) };
    assert_eq!(batch.consumed, 1);
    assert_eq!(batch.state.last_seq(), 1);
    assert_eq!(batch.acknowledgments.len(), 1);
    assert_eq!(batch.first_missing_chunk, Some(missing));
    assert_eq!(batch.stop_reason, StopReason::MissingChunk(missing));
    close(batch.state.position()[0], 15.6);
}

#[test]
fn command_order_is_stable_and_affects_the_authoritative_position() {
    let voxel_view = view();
    let initial = MovementState::new([1.0, 1.0, 1.0], 0);
    let ordered = [command(1, [0.1, 0.0, 0.0]), command(2, [-0.05, 0.0, 0.0])];
    let first = process_movement_batch(&voxel_view, initial, &ordered);
    let repeat = process_movement_batch(&voxel_view, initial, &ordered);
    let reversed = process_movement_batch(&voxel_view, initial, &[ordered[1], ordered[0]]);

    assert_eq!(first, repeat);
    assert_eq!(first.consumed, 2);
    close(first.state.position()[0], 1.05);
    // The sequence-2 command runs first; the older sequence-1 command is then
    // replayed and cannot change the result.
    close(reversed.state.position()[0], 0.95);
    assert_eq!(reversed.state.last_seq(), 2);
}

#[test]
fn negative_chunk_seam_is_reported_using_euclidean_chunk_coordinates() {
    let voxel_view = view();
    let state =
        process_movement_batch(&voxel_view, MovementState::new([0.5, 1.0, 1.0], 0), &[]).state;
    let state = process_movement_batch(&voxel_view, state, &[]).state;
    let batch = process_movement_batch(&voxel_view, state, &[command(1, [-0.25, 0.0, 0.0])]);

    assert_eq!(
        batch.first_missing_chunk,
        Some(MissingChunk { key: key(-1, 0, 0) })
    );
    assert_eq!(batch.consumed, 0);
    assert_eq!(batch.state.last_seq(), 0);
    assert_eq!(batch.state.position(), [0.5, 1.0, 1.0]);
}

#[test]
fn per_tick_work_limit_leaves_the_suffix_unresolved() {
    let voxel_view = view();
    let commands: Vec<_> = (1..=(MAX_COMMANDS_PER_TICK as u64 + 1))
        .map(|seq| command(seq, [0.0; 3]))
        .collect();
    let batch = process_movement_batch(
        &voxel_view,
        MovementState::new([1.0, 1.0, 1.0], 0),
        &commands,
    );

    assert_eq!(batch.consumed, MAX_COMMANDS_PER_TICK);
    assert_eq!(batch.acknowledgments.len(), MAX_COMMANDS_PER_TICK);
    assert_eq!(batch.state.last_seq(), MAX_COMMANDS_PER_TICK as u64);
    assert_eq!(batch.stop_reason, StopReason::WorkLimit);
}

#[test]
fn idle_credit_cannot_accumulate_without_bound() {
    let mut state = MovementState::new([1.0, 1.0, 1.0], 0);
    for _ in 0..10_000 {
        state.advance_idle_tick();
    }
    assert_eq!(state.credit_nanoblocks(), super::MAX_CREDIT);
}

#[test]
fn sustained_sixty_hz_input_does_not_accrue_float_rounding_debt() {
    let voxel_view = view();
    let mut state = MovementState::new([1.0, 1.0, 1.0], 0);
    let mut seq = 0u64;
    for tick in 0..5_000 {
        let count = if tick % 5 == 4 { 2 } else { 1 };
        let commands: Vec<_> = (0..count)
            .map(|_| {
                seq += 1;
                let direction = if seq.is_multiple_of(2) { -1.0 } else { 1.0 };
                command(seq, [direction / 6.0, 0.0, 0.0])
            })
            .collect();
        let batch = process_movement_batch(&voxel_view, state, &commands);
        assert_eq!(batch.consumed, commands.len(), "tick {tick}");
        state = batch.state;
    }
    assert_eq!(seq, 6_000);
    assert_eq!(state.last_seq(), seq);
    close(state.position()[0], 1.0);
}
