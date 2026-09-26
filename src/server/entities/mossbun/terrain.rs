use super::{EntityError, VoxelView};

// Includes the ears. A sub-block body makes corner sampling exhaustive.
const HALF_WIDTH: f32 = 0.36;
const HEIGHT: f32 = 0.94;

pub(super) fn valid_position(position: [f32; 3]) -> bool {
    position
        .iter()
        .all(|v| v.is_finite() && v.abs() < 999_999.0)
        && position[1] > crate::world::BEDROCK_Y as f32
}

fn solid(view: &VoxelView, x: f32, y: f32, z: f32) -> Result<bool, EntityError> {
    view.is_solid(x.floor() as i32, y.floor() as i32, z.floor() as i32)
        .map_err(|_| EntityError::ViewOutOfRange)
}

pub(super) fn clear(view: &VoxelView, position: [f32; 3]) -> Result<bool, EntityError> {
    for x in [position[0] - HALF_WIDTH, position[0] + HALF_WIDTH] {
        for z in [position[2] - HALF_WIDTH, position[2] + HALF_WIDTH] {
            for y in [position[1] + 0.001, position[1] + HEIGHT] {
                if solid(view, x, y, z)? {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(super) fn supported(view: &VoxelView, position: [f32; 3]) -> Result<bool, EntityError> {
    for x in [position[0] - HALF_WIDTH, position[0] + HALF_WIDTH] {
        for z in [position[2] - HALF_WIDTH, position[2] + HALF_WIDTH] {
            if !solid(view, x, position[1] - 0.01, z)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(super) fn grounded(view: &VoxelView, position: [f32; 3]) -> Result<bool, EntityError> {
    for x in [position[0] - HALF_WIDTH, position[0] + HALF_WIDTH] {
        for z in [position[2] - HALF_WIDTH, position[2] + HALF_WIDTH] {
            if solid(view, x, position[1] - 0.01, z)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(in crate::server) fn spawn_clear(
    view: &VoxelView,
    position: [f32; 3],
) -> Result<bool, EntityError> {
    Ok(valid_position(position) && clear(view, position)? && supported(view, position)?)
}
