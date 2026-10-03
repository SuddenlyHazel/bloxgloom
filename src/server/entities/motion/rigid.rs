//! Rapier runs on immutable captured obstacles; only the resulting WAL patch
//! becomes authoritative. No live simulation advances before durable receipt.
use super::{DT, solver};
use bloxgloom_host_api::motion::{Motion, MovingEntity};
use rapier3d_f64::prelude::*;

pub(super) struct Step {
    pub translation: solver::Step,
    pub orientation: [f32; 4],
    pub angular_velocity: [f32; 3],
}
pub(super) fn integrate(
    motion: Motion,
    declaration: &MovingEntity,
    position: [f64; 3],
    velocity: [f64; 3],
    colliders: &[solver::Collider],
    limits: solver::Limits,
) -> Result<Step, solver::Error> {
    let policy = declaration.physics.ok_or(solver::Error::InvalidInput)?;
    if colliders.len() > limits.colliders {
        return Err(solver::Error::ColliderCapacity);
    }
    if crate::content::moving::validate_motion(declaration, &motion).is_err()
        || declaration.validate().is_err()
        || position.iter().chain(&velocity).any(|v| !v.is_finite())
    {
        return Err(solver::Error::InvalidInput);
    }
    let mut world = PhysicsWorld::new();
    world.gravity = Vector::ZERO;
    world.integration_parameters.dt = DT;
    world.integration_parameters.max_ccd_substeps = 4;
    let rotation = Rotation::from_xyzw(
        f64::from(motion.orientation[0]),
        f64::from(motion.orientation[1]),
        f64::from(motion.orientation[2]),
        f64::from(motion.orientation[3]),
    )
    .normalize();
    let mut builder = RigidBodyBuilder::dynamic()
        .pose(Pose::from_parts(Vector::from_array(position), rotation))
        .linvel(Vector::from_array(velocity))
        .angvel(Vector::from_array(motion.angular_velocity.map(f64::from)))
        .linear_damping(f64::from(policy.linear_damping))
        .angular_damping(f64::from(policy.angular_damping))
        .can_sleep(false)
        .ccd_enabled(true);
    if policy.max_angular_speed == 0.0 {
        builder = builder.lock_rotations();
    }
    let body = world.insert_body(builder.build());
    let half = declaration.body.half_extents.map(f64::from);
    let own = world.insert_collider(
        ColliderBuilder::cuboid(half[0], half[1], half[2])
            .friction(f64::from(policy.friction))
            .friction_combine_rule(CoefficientCombineRule::Multiply)
            .restitution(
                if declaration.body.response == bloxgloom_host_api::motion::Response::Bounce {
                    f64::from(declaration.body.restitution)
                } else {
                    0.0
                },
            )
            .restitution_combine_rule(CoefficientCombineRule::Multiply)
            .build(),
        Some(body),
    );
    let mut sorted = colliders.to_vec();
    sorted.sort_by_key(|c| c.target);
    let mut targets = std::collections::HashMap::new();
    for collider in sorted {
        if (0..3).any(|a| {
            !collider.min[a].is_finite()
                || !collider.max[a].is_finite()
                || collider.min[a] >= collider.max[a]
                || !collider.displacement[a].is_finite()
        }) {
            return Err(solver::Error::InvalidInput);
        }
        let center = Vector::from_array(std::array::from_fn(|a| {
            (collider.min[a] + collider.max[a]) * 0.5
        }));
        let half = std::array::from_fn::<_, 3, _>(|a| (collider.max[a] - collider.min[a]) * 0.5);
        let obstacle = if collider.displacement == [0.0; 3] {
            None
        } else {
            Some(
                world.insert_body(
                    RigidBodyBuilder::kinematic_velocity_based()
                        .translation(center)
                        .linvel(Vector::from_array(collider.displacement) / DT)
                        .build(),
                ),
            )
        };
        let mut shape = ColliderBuilder::cuboid(half[0], half[1], half[2])
            .friction(1.0)
            .restitution(1.0);
        if obstacle.is_none() {
            shape = shape.translation(center);
        }
        let handle = world.insert_collider(shape.build(), obstacle);
        targets.insert(handle, collider.target);
    }
    world.step();
    let body = &world.bodies[body];
    let candidate = body.translation().to_array();
    // Snapshot kinematic obstacles can impart speed beyond the declaration
    // cap during the engine step. Reject the whole candidate rather than
    // publishing travel beyond the terrain envelope we actually captured.
    if Vector::from_array(candidate).distance(Vector::from_array(position))
        > f64::from(declaration.body.max_speed) * DT + 0.0001
    {
        return Err(solver::Error::MotionBudget);
    }
    let position = candidate;
    if position.iter().any(|v| !v.is_finite())
        || (0..3).any(|a| position[a] < limits.world_min[a] || position[a] > limits.world_max[a])
    {
        return Err(solver::Error::WorldBoundary);
    }
    let mut contacts = Vec::new();
    for pair in world.contact_pairs_with(own) {
        if !pair.has_any_active_contact() {
            continue;
        }
        let (other, sign) = if pair.collider1 == own {
            (pair.collider2, -1.0)
        } else {
            (pair.collider1, 1.0)
        };
        let Some(target) = targets.get(&other).copied() else {
            continue;
        };
        for manifold in pair.manifolds() {
            if manifold.data.solver_contacts.is_empty() {
                continue;
            }
            let normal = (manifold.data.normal * sign).to_array();
            contacts.push(solver::Contact {
                target,
                position,
                normal,
                incoming_velocity: velocity,
                fraction: 1.0,
            });
        }
    }
    contacts.sort_by(|a, b| {
        a.target
            .cmp(&b.target)
            .then_with(|| a.normal[0].total_cmp(&b.normal[0]))
            .then_with(|| a.normal[1].total_cmp(&b.normal[1]))
            .then_with(|| a.normal[2].total_cmp(&b.normal[2]))
    });
    let resting = contacts
        .iter()
        .find(|c| c.normal[1] > 0.5)
        .or(contacts.first())
        .map(|c| solver::ContactMemory {
            target: c.target,
            normal: c.normal,
        });
    let grounded = contacts.iter().any(|c| c.normal[1] > 0.5);
    let mut velocity = body.linvel().to_array();
    let mut angular = body.angvel().to_array();
    super::tick::clamp(&mut velocity, f64::from(declaration.body.max_speed));
    super::tick::clamp(&mut angular, f64::from(policy.max_angular_speed));
    let orientation = body.rotation().to_array().map(|v| v as f32);
    if velocity.iter().chain(&angular).any(|v| !v.is_finite())
        || orientation.iter().any(|v| !v.is_finite())
    {
        return Err(solver::Error::InvalidInput);
    }
    Ok(Step {
        translation: solver::Step {
            position,
            velocity,
            contacts,
            blocked: None,
            grounded,
            resting,
        },
        orientation,
        angular_velocity: angular.map(|v| v as f32),
    })
}

#[cfg(test)]
mod tests;
