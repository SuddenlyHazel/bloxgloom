//! Pure fixed-step swept-AABB integration over a complete captured collider set.
//! Coordinates are body centers, measured in blocks; time is seconds. The host
//! owns capture, revision fencing, masks, source exclusion, and publication.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Response {
    Stop,
    Bounce,
    Slide,
}

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub half_extents: [f64; 3],
    pub response: Response,
    pub restitution: f64,
}

/// Terrain sorts first, then players, then creatures. Ties within a kind sort
/// lexicographically by cell or exact ID; axis ties resolve X, then Y, then Z.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Target {
    Terrain { cell: [i32; 3], state: u32 },
    Player { id: u64, revision: u64 },
    Creature { id: u64, revision: u64 },
}

#[derive(Clone, Copy, Debug)]
pub struct Collider {
    pub target: Target,
    pub min: [f64; 3],
    pub max: [f64; 3],
    /// Captured displacement over the entire step, not a velocity.
    pub displacement: [f64; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct State {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub acceleration: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub target: Target,
    /// Body center at contact (surface point is center minus normal * extent).
    pub position: [f64; 3],
    pub normal: [f64; 3],
    pub incoming_velocity: [f64; 3],
    pub fraction: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    ColliderCapacity,
    SweepCapacity,
    WorldBoundary,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub colliders: usize,
    pub sweep_cells: usize,
    pub contacts: usize,
    pub world_min: [f64; 3],
    pub world_max: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    Embedded,
    ContactCapacity,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    /// Includes resting contacts for next-step suppression. The owner emits
    /// impacts only for keys/normals absent from its persisted contact state.
    pub contacts: Vec<Contact>,
    pub blocked: Option<Blocked>,
    pub grounded: bool,
}

fn finite(vector: [f64; 3]) -> bool {
    vector.iter().all(|v| v.is_finite())
}

/// Complete conservative voxel capture box. A checked cell count prevents
/// oversized sweeps from being truncated. The owner must capture every cell.
pub fn swept_cells(
    position: [f64; 3],
    displacement: [f64; 3],
    half_extents: [f64; 3],
    capacity: usize,
) -> Result<([i32; 3], [i32; 3]), Error> {
    if !finite(position)
        || !finite(displacement)
        || !finite(half_extents)
        || half_extents.iter().any(|v| *v <= 0.0)
    {
        return Err(Error::InvalidInput);
    }
    let mut min = [0; 3];
    let mut max = [0; 3];
    let mut count = 1usize;
    for axis in 0..3 {
        let end = position[axis] + displacement[axis];
        let low = (position[axis].min(end) - half_extents[axis]).floor();
        let high = (position[axis].max(end) + half_extents[axis]).floor();
        if low < i32::MIN as f64 || high > i32::MAX as f64 || !end.is_finite() {
            return Err(Error::WorldBoundary);
        }
        min[axis] = low as i32;
        max[axis] = high as i32;
        let length = (i64::from(max[axis]) - i64::from(min[axis]) + 1) as usize;
        count = count.checked_mul(length).ok_or(Error::SweepCapacity)?;
        if count > capacity {
            return Err(Error::SweepCapacity);
        }
    }
    Ok((min, max))
}

/// Semi-implicit Euler: velocity += acceleration * dt; displacement = velocity
/// * dt. Gravity is included in the captured acceleration by the adapter. All
/// slab tests use full displacement, including moving targets' relative motion.
pub fn integrate(
    state: State,
    body: Body,
    dt: f64,
    colliders: &[Collider],
    limits: Limits,
) -> Result<Step, Error> {
    integrate_inner(state, body, dt, colliders, limits, false)
}

/// Apply the first collision response and pause at the contact pose. Pending
/// impact delivery must complete before the owner integrates another step.
pub fn integrate_until_contact(
    state: State,
    body: Body,
    dt: f64,
    colliders: &[Collider],
    limits: Limits,
) -> Result<Step, Error> {
    integrate_inner(state, body, dt, colliders, limits, true)
}

fn integrate_inner(
    state: State,
    body: Body,
    dt: f64,
    colliders: &[Collider],
    limits: Limits,
    pause_at_contact: bool,
) -> Result<Step, Error> {
    if !finite(state.position)
        || !finite(state.velocity)
        || !finite(state.acceleration)
        || !finite(body.half_extents)
        || body.half_extents.iter().any(|v| *v <= 0.0)
        || !dt.is_finite()
        || dt <= 0.0
        || !body.restitution.is_finite()
        || !(0.0..=1.0).contains(&body.restitution)
        || limits.contacts == 0
        || !finite(limits.world_min)
        || !finite(limits.world_max)
        || (0..3).any(|a| limits.world_min[a] >= limits.world_max[a])
    {
        return Err(Error::InvalidInput);
    }
    if colliders.len() > limits.colliders {
        return Err(Error::ColliderCapacity);
    }
    if colliders.iter().any(|c| {
        !finite(c.min)
            || !finite(c.max)
            || !finite(c.displacement)
            || (0..3).any(|a| c.min[a] >= c.max[a])
    }) {
        return Err(Error::InvalidInput);
    }
    let velocity = std::array::from_fn(|a| state.velocity[a] + state.acceleration[a] * dt);
    let displacement = std::array::from_fn(|a| velocity[a] * dt);
    swept_cells(
        state.position,
        displacement,
        body.half_extents,
        limits.sweep_cells,
    )?;
    if (0..3).any(|a| {
        state.position[a].min(state.position[a] + displacement[a]) - body.half_extents[a]
            < limits.world_min[a]
            || state.position[a].max(state.position[a] + displacement[a]) + body.half_extents[a]
                > limits.world_max[a]
    }) {
        return Err(Error::WorldBoundary);
    }
    let mut result = Step {
        position: state.position,
        velocity,
        contacts: Vec::new(),
        blocked: None,
        grounded: false,
    };
    let mut elapsed = 0.0;
    for iteration in 0..=limits.contacts {
        let remaining = 1.0 - elapsed;
        let movement = std::array::from_fn(|a| result.velocity[a] * dt * remaining);
        let mut earliest: Option<(f64, [f64; 3], bool, &Collider)> = None;
        for collider in colliders {
            if let Some((time, normal, embedded)) = sweep(
                result.position,
                body.half_extents,
                movement,
                collider,
                elapsed,
                remaining,
            ) && earliest.as_ref().is_none_or(|old| {
                time.total_cmp(&old.0).is_lt() || (time == old.0 && collider.target < old.3.target)
            }) {
                earliest = Some((time, normal, embedded, collider));
            }
        }
        let Some((time, normal, embedded, collider)) = earliest else {
            for (axis, value) in movement.iter().enumerate() {
                result.position[axis] += value;
            }
            check_bounds(result.position, body, limits)?;
            break;
        };
        if iteration == limits.contacts {
            result.velocity = [0.0; 3];
            result.blocked = Some(Blocked::ContactCapacity);
            break;
        }
        for (axis, value) in movement.iter().enumerate() {
            result.position[axis] += value * time;
        }
        check_bounds(result.position, body, limits)?;
        elapsed += time * remaining;
        result.contacts.push(Contact {
            target: collider.target,
            position: result.position,
            normal,
            incoming_velocity: result.velocity,
            fraction: elapsed,
        });
        result.grounded |= normal[1] > 0.0;
        if embedded {
            result.velocity = [0.0; 3];
            result.blocked = Some(Blocked::Embedded);
            break;
        }
        if body.response == Response::Stop {
            result.velocity = [0.0; 3];
            break;
        }
        // Reflect/remove the normal component in the target's moving frame.
        let target_velocity: [f64; 3] = std::array::from_fn(|a| collider.displacement[a] / dt);
        let inward: f64 = (0..3)
            .map(|a| (result.velocity[a] - target_velocity[a]) * normal[a])
            .sum();
        let factor = if body.response == Response::Bounce {
            1.0 + body.restitution
        } else {
            1.0
        };
        for (axis, value) in normal.iter().enumerate() {
            result.velocity[axis] -= factor * inward * value;
        }
        if pause_at_contact || elapsed >= 1.0 {
            break;
        }
    }
    Ok(result)
}

fn check_bounds(position: [f64; 3], body: Body, limits: Limits) -> Result<(), Error> {
    if (0..3).any(|a| {
        !position[a].is_finite()
            || position[a] - body.half_extents[a] < limits.world_min[a]
            || position[a] + body.half_extents[a] > limits.world_max[a]
    }) {
        Err(Error::WorldBoundary)
    } else {
        Ok(())
    }
}

fn sweep(
    position: [f64; 3],
    half: [f64; 3],
    movement: [f64; 3],
    collider: &Collider,
    elapsed: f64,
    remaining: f64,
) -> Option<(f64, [f64; 3], bool)> {
    let min: [f64; 3] =
        std::array::from_fn(|a| collider.min[a] + collider.displacement[a] * elapsed - half[a]);
    let max: [f64; 3] =
        std::array::from_fn(|a| collider.max[a] + collider.displacement[a] * elapsed + half[a]);
    if (0..3).all(|a| position[a] > min[a] && position[a] < max[a]) {
        return Some((0.0, [0.0; 3], true));
    }
    let relative: [f64; 3] =
        std::array::from_fn(|a| movement[a] - collider.displacement[a] * remaining);
    let mut enter = f64::NEG_INFINITY;
    let mut exit = f64::INFINITY;
    let mut normal = [0.0; 3];
    for axis in 0..3 {
        if relative[axis] == 0.0 {
            // Tangent contact alone does not prevent sliding along a face.
            if position[axis] <= min[axis] || position[axis] >= max[axis] {
                return None;
            }
            continue;
        }
        let t1 = (min[axis] - position[axis]) / relative[axis];
        let t2 = (max[axis] - position[axis]) / relative[axis];
        let near = t1.min(t2);
        if near > enter {
            enter = near;
            normal = [0.0; 3];
            normal[axis] = if relative[axis] > 0.0 { -1.0 } else { 1.0 };
        }
        exit = exit.min(t1.max(t2));
    }
    if enter > exit || exit < 0.0 || !(0.0..=1.0).contains(&enter) {
        None
    } else {
        Some((enter, normal, false))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
