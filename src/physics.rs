//! Shared player collision geometry for server movement and client prediction.

// Authenticated movement normally takes far fewer than 64 steps per axis.
const MAX_MOVEMENT_STEPS_PER_AXIS: f32 = 64.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolveError<E> {
    Missing(E),
    InvalidCoordinates,
    OutOfBounds,
}

/// Resolve X/Z/Y motion against authoritative or locally streamed voxels.
/// A missing voxel aborts the entire prediction rather than inventing terrain.
pub(crate) fn resolve_player_movement<E>(
    mut position: [f32; 3],
    delta: [f32; 3],
    mut solid: impl FnMut(i32, i32, i32) -> Result<bool, E>,
) -> Result<[f32; 3], ResolveError<E>> {
    if position
        .iter()
        .chain(delta.iter())
        .any(|coordinate| !coordinate.is_finite())
    {
        return Err(ResolveError::InvalidCoordinates);
    }

    for axis in [0, 2, 1] {
        let step_count = (delta[axis].abs() / 0.25).ceil().max(1.0);
        if step_count > MAX_MOVEMENT_STEPS_PER_AXIS {
            return Err(ResolveError::OutOfBounds);
        }
        let steps = step_count as usize;
        let step = delta[axis] / steps as f32;
        for _ in 0..steps {
            let mut candidate = position;
            candidate[axis] += step;
            if candidate.iter().any(|value| value.abs() >= 1_000_000.0) {
                break;
            }
            if player_collides(candidate, &mut solid).map_err(ResolveError::Missing)? {
                break;
            }
            position = candidate;
        }
    }
    Ok(position)
}

/// The player's 0.6-wide, 1.7-tall hitbox, sampled identically on both sides.
pub(crate) fn player_collides<E>(
    feet: [f32; 3],
    mut solid: impl FnMut(i32, i32, i32) -> Result<bool, E>,
) -> Result<bool, E> {
    for x in [feet[0] - 0.3, feet[0] + 0.3] {
        for y in [feet[1] + 0.05, feet[1] + 0.9, feet[1] + 1.75] {
            for z in [feet[2] - 0.3, feet[2] + 0.3] {
                if solid(x.floor() as i32, y.floor() as i32, z.floor() as i32)? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
