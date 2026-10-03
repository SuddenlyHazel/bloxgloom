//! Deterministic, bounded movement processing for one player's input batch.
//!
//! The coordinator calls this once per 20 ms simulation tick with commands in
//! arrival order and an immutable voxel view. Unavailable chunks leave that
//! command and every later command unresolved so the coordinator can load the
//! missing authoritative data before retrying.

use super::voxel_view::{MissingChunk, MovementError, VoxelView};
use bloxgloom_host_api::player::PlayerRules;
use std::time::Duration;

mod coordinator;
mod ground;
pub(super) use ground::set_flying;
pub(in crate::server) mod sprint;
mod stance;
pub(super) use stance::clear as clear_stance;
mod teleport;
pub(super) use coordinator::advance_players;
pub(super) use teleport::{Reset, ready as movement_ready, teleport};

/// Work executed by the player-movement worker pool in one simulation tick.
/// The capacity denominator spans dispatch through the completed barrier;
/// it does not include chunk loading, networking, or other server workers.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WorkerLoad {
    pub(super) busy: Duration,
    pub(super) capacity: Duration,
}

const TICK_MILLIS: f64 = 20.0;
const CREDIT_SCALE: f64 = 1_000_000_000.0;
// The client sends f32 displacements. A tiny fixed tolerance absorbs f32
// normalization/rounding without granting useful speed (at most 0.0000128
// blocks/s), while nanoblock accounting prevents per-command quantization
// from accumulating into visible movement debt.
const FLOAT_ROUNDING_ALLOWANCE_PER_TICK: u32 = 256;
fn credit_per_tick(rules: PlayerRules) -> u32 {
    (rules.motion().budget_blocks_per_second * TICK_MILLIS / 1_000.0 * CREDIT_SCALE) as u32
        + FLOAT_ROUNDING_ALLOWANCE_PER_TICK
}

fn max_credit(rules: PlayerRules) -> u32 {
    (rules.motion().budget_blocks_per_second * 0.250 * CREDIT_SCALE) as u32
}

/// Maximum number of commands (including replays and rejected inputs) handled
/// for one player in a tick. Remaining commands stay queued for the next tick.
pub const MAX_COMMANDS_PER_TICK: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementState {
    position: [f32; 3],
    last_seq: u64,
    credit_nanoblocks: u32,
    crouching: bool,
    requested_crouch: bool,
    flying: bool,
    sprinting: bool,
    vertical_velocity: f32,
    jump_requested: bool,
}

impl MovementState {
    pub fn new(position: [f32; 3], last_seq: u64) -> Self {
        Self {
            position,
            last_seq,
            credit_nanoblocks: 0,
            crouching: false,
            requested_crouch: false,
            flying: true,
            sprinting: false,
            vertical_velocity: 0.0,
            jump_requested: false,
        }
    }

    #[inline]
    pub fn position(self) -> [f32; 3] {
        self.position
    }

    pub fn crouching(self) -> bool {
        self.crouching
    }
    pub fn flying(self) -> bool {
        self.flying
    }
    pub(super) fn request_jump(&mut self) {
        if !self.flying {
            self.jump_requested = true;
        }
    }
    pub(super) fn stance_pending(self) -> bool {
        self.crouching != self.requested_crouch
    }
    pub(super) fn request_crouch(&mut self, crouching: bool) {
        self.requested_crouch = crouching;
    }

    #[inline]
    pub fn last_seq(self) -> u64 {
        self.last_seq
    }

    /// Remaining movement allowance in billionths of a block.
    #[inline]
    #[cfg(test)]
    pub fn credit_nanoblocks(self) -> u32 {
        self.credit_nanoblocks
    }

    /// Advances an idle player without constructing a voxel view or worker
    /// job. The coordinator calls exactly one of this or
    /// `process_movement_batch` for each player on each tick.
    pub fn advance_idle_tick(&mut self, rules: PlayerRules) {
        let rules = rules.for_movement(self.crouching, self.sprinting);
        self.credit_nanoblocks = self
            .credit_nanoblocks
            .saturating_add(credit_per_tick(rules))
            .min(max_credit(rules));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementCommand {
    pub seq: u64,
    pub delta: [f32; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AckKind {
    /// A valid command was resolved against the captured voxel view.
    Resolved,
    /// The sequence was already consumed, or its delta was invalid/excessive.
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementAck {
    pub seq: u64,
    pub position: [f32; 3],
    pub kind: AckKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    /// All supplied commands were consumed.
    InputDrained,
    /// The next valid command exceeds the current movement allowance.
    MovementBudget,
    /// Collision sampling reached a chunk absent from the immutable view.
    MissingChunk(MissingChunk),
    /// The per-player work limit was reached; remaining inputs stay queued.
    WorkLimit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovementBatch {
    pub state: MovementState,
    pub acknowledgments: Vec<MovementAck>,
    /// Number of leading commands consumed from the input slice.
    pub consumed: usize,
    /// First unavailable chunk, if collision resolution deferred a command.
    pub first_missing_chunk: Option<MissingChunk>,
    pub stop_reason: StopReason,
}

/// Adds one fixed tick of movement credit and consumes an ordered command
/// prefix. Credit is granted once per call, never once per command. A valid
/// command that cannot yet fit in the remaining allowance and all commands
/// after it are left unresolved.
pub fn process_movement_batch(
    view: &VoxelView,
    mut state: MovementState,
    commands: &[MovementCommand],
) -> MovementBatch {
    if let Err(missing) = stance::resolve(view, &mut state) {
        return MovementBatch {
            state,
            acknowledgments: vec![],
            consumed: 0,
            first_missing_chunk: Some(missing),
            stop_reason: StopReason::MissingChunk(missing),
        };
    }
    state.advance_idle_tick(view.player_rules());
    let mut batch = process_commands(view, state, commands);
    if !batch.state.flying
        && let Err(missing) = ground::advance(view, &mut batch.state)
    {
        batch.first_missing_chunk = Some(missing);
        batch.stop_reason = StopReason::MissingChunk(missing);
    }
    if let Some(ack) = batch.acknowledgments.last_mut() {
        ack.position = batch.state.position;
    }
    batch
}

fn process_commands(
    view: &VoxelView,
    mut state: MovementState,
    commands: &[MovementCommand],
) -> MovementBatch {
    let rules = view
        .player_rules()
        .for_movement(state.crouching, state.sprinting);

    let work_count = commands.len().min(MAX_COMMANDS_PER_TICK);
    let mut acknowledgments = Vec::with_capacity(work_count);
    let mut consumed = 0;

    for command in commands.iter().take(work_count).copied() {
        let position = state.position;
        if command.seq <= state.last_seq {
            consumed += 1;
            continue;
        }

        let Some(cost) = movement_cost(rules, command.delta) else {
            state.last_seq = command.seq;
            acknowledgments.push(MovementAck {
                seq: command.seq,
                position,
                kind: AckKind::Rejected,
            });
            consumed += 1;
            continue;
        };

        if cost > state.credit_nanoblocks {
            return MovementBatch {
                state,
                acknowledgments,
                consumed,
                first_missing_chunk: None,
                stop_reason: StopReason::MovementBudget,
            };
        }

        let mut body = rules.body();
        let mut delta = command.delta;
        if !state.flying {
            body.foot_inset = 0.0;
            delta[1] = 0.0;
        }
        let next_position =
            match super::voxel_view::resolve_player_movement_with_body(view, body, position, delta)
            {
                Ok(position) => position,
                Err(MovementError::MissingChunk(missing)) => {
                    return MovementBatch {
                        state,
                        acknowledgments,
                        consumed,
                        first_missing_chunk: Some(missing),
                        stop_reason: StopReason::MissingChunk(missing),
                    };
                }
                Err(MovementError::InvalidCoordinates | MovementError::OutOfBounds) => {
                    state.last_seq = command.seq;
                    acknowledgments.push(MovementAck {
                        seq: command.seq,
                        position,
                        kind: AckKind::Rejected,
                    });
                    consumed += 1;
                    continue;
                }
            };

        // Charge requested distance even if a wall prevents all or part of the
        // movement. Repeatedly pushing into a wall must not preserve allowance.
        state.credit_nanoblocks -= cost;
        state.position = next_position;
        state.last_seq = command.seq;
        acknowledgments.push(MovementAck {
            seq: command.seq,
            position: next_position,
            kind: AckKind::Resolved,
        });
        consumed += 1;
    }

    let stop_reason = if commands.len() > work_count {
        StopReason::WorkLimit
    } else {
        StopReason::InputDrained
    };
    MovementBatch {
        state,
        acknowledgments,
        consumed,
        first_missing_chunk: None,
        stop_reason,
    }
}

/// Converts a finite, bounded movement vector to conservative fixed-point
/// distance. Rounding upward ensures quantization can never grant extra speed.
fn movement_cost(rules: PlayerRules, delta: [f32; 3]) -> Option<u32> {
    if delta.iter().any(|component| !component.is_finite()) {
        return None;
    }
    let [x, y, z] = delta.map(f64::from);
    let distance = (x * x + y * y + z * z).sqrt();
    if distance > max_credit(rules) as f64 / CREDIT_SCALE {
        return None;
    }
    Some((distance * CREDIT_SCALE).ceil() as u32)
}

#[cfg(test)]
mod tests;
