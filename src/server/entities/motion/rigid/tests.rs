use super::*;
use bloxgloom_host_api::{
    RegistrationError,
    gameplay::EntityState,
    motion::{Body, CollisionMask, Physics, Response},
};
use std::time::Instant;
struct Bytes;
impl EntityState for Bytes {
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn public(&self, data: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(data.to_vec())
    }
}
fn declaration() -> MovingEntity {
    MovingEntity {
        key: "test:rigid".into(),
        schema_version: 1,
        schema_fingerprint: 1,
        max_state_bytes: 16,
        max_public_bytes: 16,
        body: Body {
            half_extents: [0.3, 0.15, 0.1],
            collisions: CollisionMask {
                terrain: true,
                players: false,
                creatures: false,
            },
            response: Response::Slide,
            restitution: 0.0,
            gravity_scale: 1.0,
            max_speed: 64.0,
            max_acceleration: 128.0,
        },
        physics: Some(Physics::default()),
        lifetime_ticks: 1000,
        interval: 10,
        source_exclusion_ticks: 0,
        handles_impact: false,
        handles_expiry: false,
        model: vec![],
        state: std::sync::Arc::new(Bytes),
    }
}
fn motion() -> Motion {
    Motion {
        position: [0.0, 2.0, 0.0],
        velocity: [2.0, 0.0, 0.0],
        acceleration: [0.0; 3],
        orientation: [0.0, 0.0, 0.0, 1.0],
        angular_velocity: [0.0, 2.0, 0.0],
        revision: 0,
        grounded: false,
    }
}
fn limits() -> solver::Limits {
    solver::Limits {
        colliders: 64,
        sweep_cells: 4096,
        contacts: 4,
        world_min: [-100.0; 3],
        world_max: [100.0; 3],
    }
}
fn advance(
    mut motion: Motion,
    d: &MovingEntity,
    colliders: &[solver::Collider],
    ticks: usize,
    gravity: bool,
) -> Motion {
    for _ in 0..ticks {
        let mut velocity = motion.velocity.map(f64::from);
        if gravity {
            velocity[1] -= 20.0 * DT;
        }
        let step = integrate(
            motion,
            d,
            motion.position.map(f64::from),
            velocity,
            colliders,
            limits(),
        )
        .unwrap();
        motion.position = step.translation.position.map(|v| v as f32);
        motion.velocity = step.translation.velocity.map(|v| v as f32);
        motion.angular_velocity = step.angular_velocity;
        motion.orientation = step.orientation;
        motion.grounded = step.translation.grounded;
    }
    motion
}
#[test]
fn rapier_drag_and_angular_damping_use_committed_finite_rotation() {
    let mut d = declaration();
    d.physics = Some(Physics {
        linear_damping: 2.0,
        angular_damping: 3.0,
        ..Default::default()
    });
    let initial = motion();
    let end = advance(initial, &d, &[], 25, false);
    assert!(end.velocity[0] > 0.0 && end.velocity[0] < initial.velocity[0] * 0.2);
    assert!(
        end.angular_velocity[1] > 0.0
            && end.angular_velocity[1] < initial.angular_velocity[1] * 0.1
    );
    assert!(end.orientation[1].abs() > 0.1);
    assert!(end.validate().is_ok());
    // Restart reconstructs exactly the committed canonical state, without solver caches.
    let a = advance(initial, &d, &[], 12, false);
    let resumed = advance(a, &d, &[], 13, false);
    assert_eq!(end, resumed);
}
#[test]
fn rapier_rotated_contacts_friction_and_ccd_stop_terrain_penetration() {
    let floor = solver::Collider {
        target: solver::Target::Terrain {
            cell: [0, 0, 0],
            state: 1,
        },
        min: [-20.0, -1.0, -20.0],
        max: [20.0, 0.0, 20.0],
        displacement: [0.0; 3],
    };
    let mut initial = motion();
    initial.angular_velocity = [0.0; 3];
    initial.position = [0.0, 0.3, 0.0];
    initial.orientation = Rotation::from_rotation_z(std::f64::consts::FRAC_PI_4)
        .to_array()
        .map(|v| v as f32);
    let mut d = declaration();
    d.physics.as_mut().unwrap().max_angular_speed = 0.0;
    let friction = advance(initial, &d, &[floor], 100, true);
    assert!(friction.position[1] > 0.1 && friction.position[1] < 0.5);
    assert!(friction.velocity[0].abs() < 0.1, "{:?}", friction);
    d.physics.as_mut().unwrap().friction = 0.0;
    let free = advance(initial, &d, &[floor], 100, true);
    assert!(free.velocity[0] > 1.5);
    let wall = solver::Collider {
        target: solver::Target::Terrain {
            cell: [1, 0, 0],
            state: 1,
        },
        min: [1.0, -3.0, -3.0],
        max: [1.05, 3.0, 3.0],
        displacement: [0.0; 3],
    };
    initial.position = [0.0, 1.0, 0.0];
    initial.velocity = [64.0, 0.0, 0.0];
    let impact = advance(initial, &d, &[wall], 1, false);
    assert!(impact.position[0] < 1.0, "{:?}", impact);
}
#[test]
fn rapier_policy_and_commands_reject_unsafe_combinations_and_caps() {
    let mut d = declaration();
    d.handles_impact = true;
    assert!(d.validate().is_err());
    d.handles_impact = false;
    d.physics.as_mut().unwrap().linear_damping = f32::NAN;
    assert!(d.validate().is_err());
    d.physics.as_mut().unwrap().linear_damping = 0.0;
    d.physics.as_mut().unwrap().max_angular_speed = 1.0;
    let m = motion();
    assert!(
        integrate(
            m,
            &d,
            m.position.map(f64::from),
            m.velocity.map(f64::from),
            &[],
            limits()
        )
        .is_err(),
        "input must respect declared angular cap before integration"
    );
    let mut m = motion();
    m.angular_velocity = [9.0, 0.0, 0.0];
    assert!(
        integrate(
            m,
            &d,
            m.position.map(f64::from),
            m.velocity.map(f64::from),
            &[],
            limits()
        )
        .is_err()
    );
}

#[test]
fn rapier_kinematic_impulses_cannot_publish_outside_captured_motion_budget() {
    let mut d = declaration();
    d.body.max_speed = 0.1;
    let mut m = motion();
    m.velocity = [0.0; 3];
    m.angular_velocity = [0.0; 3];
    let collider = solver::Collider {
        target: solver::Target::Player { id: 1, revision: 1 },
        min: [-0.7, 1.0, -1.0],
        max: [-0.3, 3.0, 1.0],
        displacement: [1.0, 0.0, 0.0],
    };
    assert!(matches!(
        integrate(
            m,
            &d,
            m.position.map(f64::from),
            [0.0; 3],
            &[collider],
            limits()
        ),
        Err(solver::Error::MotionBudget)
    ));
}

#[test]
#[ignore = "manual release CPU measurement; reconstructs a bounded Rapier world per committed step"]
fn rapier_bounded_snapshot_step_benchmark() {
    let d = declaration();
    let m = motion();
    for count in [0_usize, 16, 64] {
        let colliders = (0..count)
            .map(|i| solver::Collider {
                target: solver::Target::Terrain {
                    cell: [i as i32, 0, 0],
                    state: 1,
                },
                min: [i as f64 - 8.0, -1.0, -0.5],
                max: [i as f64 - 7.0, 0.0, 0.5],
                displacement: [0.0; 3],
            })
            .collect::<Vec<_>>();
        let start = Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(
                integrate(
                    m,
                    &d,
                    m.position.map(f64::from),
                    m.velocity.map(f64::from),
                    &colliders,
                    solver::Limits {
                        colliders: 128,
                        ..limits()
                    },
                )
                .unwrap(),
            );
        }
        eprintln!(
            "rapier snapshot step colliders={count}: {:.3} us/body, 64-body frame {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.0,
            start.elapsed().as_secs_f64() * 64.0
        );
    }
}
