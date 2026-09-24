//! Exact grid traversal for selecting voxels with a ray.

use crate::world::{self, TALL_GRASS};
use glam::Vec3;

/// The longest ray the client will cast. The server currently accepts edits
/// from within eight blocks, measured from the player's eye to the block
/// center, so this traversal does not offer a longer client reach.
pub const MAX_REACH: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    NegX,
    PosX,
    NegY,
    PosY,
    NegZ,
    PosZ,
}

impl Face {
    pub const fn normal(self) -> [i32; 3] {
        match self {
            Self::NegX => [-1, 0, 0],
            Self::PosX => [1, 0, 0],
            Self::NegY => [0, -1, 0],
            Self::PosY => [0, 1, 0],
            Self::NegZ => [0, 0, -1],
            Self::PosZ => [0, 0, 1],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Coordinates of the non-air voxel intersected by the ray.
    pub block: [i32; 3],
    /// The cell one voxel out from `face`, suitable for placement.
    pub adjacent: [i32; 3],
    pub block_id: u8,
    /// Distance from `origin` in world units.
    pub distance: f32,
    /// The outward-facing side of `block` crossed by the ray.
    pub face: Face,
}

/// Finds the first non-air voxel on a ray using exact voxel-grid traversal.
///
/// Voxels use half-open bounds (`[x,x+1)` on every axis). Thus a ray on an
/// integer plane moving in the negative direction enters the lower cell at
/// distance zero; on a plane parallel to the ray, the higher-coordinate cell
/// owns the boundary. At simultaneous edge or corner crossings all tied axes
/// advance together, so cells touched only at a seam are not treated as hits.
/// An unavailable sample (`None`) ends the query just like an unloaded chunk.
pub fn raycast(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    mut sample: impl FnMut(i32, i32, i32) -> Option<u8>,
) -> Option<Hit> {
    if !origin.is_finite()
        || !direction.is_finite()
        || !max_distance.is_finite()
        || max_distance < 0.0
    {
        return None;
    }

    let raw_direction = [
        f64::from(direction.x),
        f64::from(direction.y),
        f64::from(direction.z),
    ];
    let length = (raw_direction[0] * raw_direction[0]
        + raw_direction[1] * raw_direction[1]
        + raw_direction[2] * raw_direction[2])
        .sqrt();
    if length == 0.0 || !length.is_finite() {
        return None;
    }
    let direction = [
        raw_direction[0] / length,
        raw_direction[1] / length,
        raw_direction[2] / length,
    ];
    let origin = [
        f64::from(origin.x),
        f64::from(origin.y),
        f64::from(origin.z),
    ];
    let reach = f64::from(max_distance.min(MAX_REACH));

    let mut cell = [0_i32; 3];
    for axis in 0..3 {
        let floored = origin[axis].floor();
        if floored < f64::from(i32::MIN) || floored > f64::from(i32::MAX) {
            return None;
        }
        cell[axis] = floored as i32;
    }

    // The floor rule assigns an integer boundary to the higher cell. If the
    // ray immediately travels toward lower coordinates, begin in the cell it
    // enters for t > 0 instead of incorrectly testing the cell behind it.
    let mut initial_face = None;
    for axis in 0..3 {
        if direction[axis] < 0.0 && origin[axis] == origin[axis].floor() {
            cell[axis] = cell[axis].checked_sub(1)?;
            if initial_face.is_none() {
                initial_face = Some(positive_face(axis));
            }
        }
    }

    let initial_block = sample(cell[0], cell[1], cell[2])?;
    if initial_block != 0 && !world::is_plant(initial_block) {
        let face = initial_face.unwrap_or_else(|| nearest_face(origin, cell));
        return make_hit(cell, initial_block, 0.0, face);
    } else if world::is_plant(initial_block)
        && let Some((distance, face)) = plant_intersection(origin, direction, cell, initial_block)
        && distance <= reach
    {
        return make_hit(cell, initial_block, distance, face);
    }

    let mut step = [0_i32; 3];
    let mut t_delta = [f64::INFINITY; 3];
    let mut t_max = [f64::INFINITY; 3];
    for axis in 0..3 {
        if direction[axis] > 0.0 {
            step[axis] = 1;
            t_delta[axis] = 1.0 / direction[axis];
            t_max[axis] = (f64::from(cell[axis]) + 1.0 - origin[axis]) / direction[axis];
        } else if direction[axis] < 0.0 {
            step[axis] = -1;
            t_delta[axis] = -1.0 / direction[axis];
            t_max[axis] = (origin[axis] - f64::from(cell[axis])) / -direction[axis];
        }
    }

    loop {
        let distance = t_max[0].min(t_max[1]).min(t_max[2]);
        if !distance.is_finite() || distance > reach {
            return None;
        }

        let mut crossed_axis = None;
        for axis in 0..3 {
            if same_crossing(t_max[axis], distance) {
                if crossed_axis.is_none() {
                    crossed_axis = Some(axis);
                }
                cell[axis] = cell[axis].checked_add(step[axis])?;
                t_max[axis] += t_delta[axis];
            }
        }
        let axis = crossed_axis?;
        let face = if step[axis] > 0 {
            negative_face(axis)
        } else {
            positive_face(axis)
        };
        let block_id = sample(cell[0], cell[1], cell[2])?;
        if block_id != 0 && !world::is_plant(block_id) {
            return make_hit(cell, block_id, distance, face);
        } else if world::is_plant(block_id)
            && let Some((plant_distance, plant_face)) =
                plant_intersection(origin, direction, cell, block_id)
            && plant_distance <= reach
        {
            return make_hit(cell, block_id, plant_distance, plant_face);
        }
    }
}

/// A narrow selectable center lets rays pass the edges of decorative plants.
/// Their visual crossed quads remain easy to target near the stem.
fn plant_intersection(
    origin: [f64; 3],
    direction: [f64; 3],
    cell: [i32; 3],
    block: u8,
) -> Option<(f64, Face)> {
    let margin = if block == TALL_GRASS { 0.35 } else { 0.22 };
    let lower = [margin, 0.0, margin];
    let upper = [1.0 - margin, 0.9, 1.0 - margin];
    let mut enter = f64::NEG_INFINITY;
    let mut leave = f64::INFINITY;
    let mut face = Face::PosY;
    for axis in 0..3 {
        let min = f64::from(cell[axis]) + lower[axis];
        let max = f64::from(cell[axis]) + upper[axis];
        if direction[axis] == 0.0 {
            if origin[axis] < min || origin[axis] > max {
                return None;
            }
            continue;
        }
        let near = (min - origin[axis]) / direction[axis];
        let far = (max - origin[axis]) / direction[axis];
        let (near, far, near_face) = if near <= far {
            (near, far, negative_face(axis))
        } else {
            (far, near, positive_face(axis))
        };
        if near > enter {
            enter = near;
            face = near_face;
        }
        leave = leave.min(far);
    }
    (leave >= enter && leave >= 0.0).then_some((enter.max(0.0), face))
}

fn make_hit(block: [i32; 3], block_id: u8, distance: f64, face: Face) -> Option<Hit> {
    let normal = face.normal();
    Some(Hit {
        block,
        adjacent: [
            block[0].checked_add(normal[0])?,
            block[1].checked_add(normal[1])?,
            block[2].checked_add(normal[2])?,
        ],
        block_id,
        distance: distance as f32,
        face,
    })
}

fn negative_face(axis: usize) -> Face {
    match axis {
        0 => Face::NegX,
        1 => Face::NegY,
        _ => Face::NegZ,
    }
}

fn positive_face(axis: usize) -> Face {
    match axis {
        0 => Face::PosX,
        1 => Face::PosY,
        _ => Face::PosZ,
    }
}

fn nearest_face(origin: [f64; 3], cell: [i32; 3]) -> Face {
    let mut nearest = (f64::INFINITY, Face::NegX);
    for axis in 0..3 {
        let fraction = origin[axis] - f64::from(cell[axis]);
        let lower = fraction;
        if lower < nearest.0 {
            nearest = (lower, negative_face(axis));
        }
        let upper = 1.0 - fraction;
        if upper < nearest.0 {
            nearest = (upper, positive_face(axis));
        }
    }
    nearest.1
}

fn same_crossing(a: f64, b: f64) -> bool {
    // Inputs arrive as f32; this tolerance only absorbs rounding in the
    // normalized direction and does not blur neighboring f32-sized events.
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1.0e-12 * a.abs().max(b.abs()).max(1.0)
}

#[cfg(test)]
#[path = "raycast/tests.rs"]
mod tests;
