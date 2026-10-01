use super::*;

fn limits() -> Limits {
    Limits {
        colliders: 256,
        sweep_cells: 8192,
        contacts: 8,
        world_min: [-1000.0; 3],
        world_max: [1000.0; 3],
    }
}
fn body(response: Response) -> Body {
    Body {
        half_extents: [0.25; 3],
        response,
        restitution: 1.0,
    }
}
fn state(position: [f64; 3], velocity: [f64; 3]) -> State {
    State {
        position,
        velocity,
        acceleration: [0.0; 3],
    }
}
fn voxel(cell: [i32; 3]) -> Collider {
    Collider {
        target: Target::Terrain { cell, state: 1 },
        min: cell.map(f64::from),
        max: cell.map(|v| f64::from(v) + 1.0),
        displacement: [0.0; 3],
    }
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn fast_projectile_hits_thin_wall_instead_of_crossing_it() {
    let step = integrate(
        state([0.0, 0.5, 0.5], [500.0, 0.0, 0.0]),
        body(Response::Stop),
        0.02,
        &[voxel([5, 0, 0])],
        limits(),
    )
    .unwrap();
    assert_eq!(step.position, [4.75, 0.5, 0.5]);
    assert_eq!(step.velocity, [0.0; 3]);
    assert_eq!(step.contacts[0].normal, [-1.0, 0.0, 0.0]);
    near(step.contacts[0].fraction, 0.475);
}

#[test]
fn moving_targets_cross_even_when_endpoints_do_not_overlap() {
    let mut target = voxel([8, 0, 0]);
    target.target = Target::Player {
        id: 3,
        revision: 42,
    };
    target.displacement = [-10.0, 0.0, 0.0];
    let step = integrate(
        state([0.0, 0.5, 0.5], [500.0, 0.0, 0.0]),
        body(Response::Stop),
        0.02,
        &[target],
        limits(),
    )
    .unwrap();
    near(step.position[0], 3.875);
    near(step.contacts[0].fraction, 0.3875);
    assert_eq!(step.contacts[0].target, target.target);
}

#[test]
fn response_uses_target_velocity_and_pauses_pending_impact() {
    let mut target = voxel([5, 0, 0]);
    target.displacement = [-1.0, 0.0, 0.0];
    let step = integrate_until_contact(
        state([0.0, 0.5, 0.5], [500.0, 0.0, 0.0]),
        body(Response::Bounce),
        0.02,
        &[target],
        limits(),
    )
    .unwrap();
    near(step.position[0], 10.0 * 4.75 / 11.0);
    near(step.velocity[0], -600.0);
    assert_eq!(step.contacts.len(), 1);
    assert_eq!(step.blocked, None);
}

#[test]
fn equal_time_contacts_have_stable_target_and_axis_order() {
    let terrain = voxel([5, 0, 0]);
    let mut entity = terrain;
    entity.target = Target::Creature { id: 1, revision: 1 };
    let initial = state([0.0, 0.5, 0.5], [500.0, 0.0, 0.0]);
    for colliders in [[entity, terrain], [terrain, entity]] {
        let step = integrate(initial, body(Response::Stop), 0.02, &colliders, limits()).unwrap();
        assert_eq!(step.contacts[0].target, terrain.target);
    }
    let corner = integrate(
        state([0.0, 0.0, 0.5], [100.0, 100.0, 0.0]),
        body(Response::Stop),
        0.02,
        &[voxel([1, 1, 0])],
        limits(),
    )
    .unwrap();
    assert_eq!(corner.contacts[0].normal, [-1.0, 0.0, 0.0]);
}

#[test]
fn slide_keeps_tangent_movement_and_rest_does_not_repeat_impact() {
    let step = integrate(
        state([0.0, 1.0, 0.5], [50.0, -100.0, 0.0]),
        body(Response::Slide),
        0.02,
        &[voxel([0, -1, 0]), voxel([1, -1, 0])],
        limits(),
    )
    .unwrap();
    near(step.position[0], 1.0);
    near(step.position[1], 0.25);
    assert!(step.grounded);
    assert_eq!(step.velocity, [50.0, 0.0, 0.0]);
    let resting = integrate(
        state([0.5, 0.25, 0.5], [0.0; 3]),
        body(Response::Slide),
        0.02,
        &[voxel([0, -1, 0])],
        limits(),
    )
    .unwrap();
    assert!(resting.contacts.is_empty());
}

#[test]
fn gravity_contact_is_suppressible_using_persisted_target_and_normal() {
    let mut falling = state([0.5, 0.25, 0.5], [0.0; 3]);
    falling.acceleration = [0.0, -20.0, 0.0];
    let a = integrate(
        falling,
        body(Response::Slide),
        0.02,
        &[voxel([0, -1, 0])],
        limits(),
    )
    .unwrap();
    let b = integrate(
        falling,
        body(Response::Slide),
        0.02,
        &[voxel([0, -1, 0])],
        limits(),
    )
    .unwrap();
    assert_eq!(
        (a.contacts[0].target, a.contacts[0].normal),
        (b.contacts[0].target, b.contacts[0].normal)
    );
    assert!(a.grounded);
}

#[test]
fn terrain_edit_embedding_body_stops_without_tunneling_out() {
    let step = integrate(
        state([0.5; 3], [100.0; 3]),
        body(Response::Bounce),
        0.02,
        &[voxel([0; 3])],
        limits(),
    )
    .unwrap();
    assert_eq!(step.position, [0.5; 3]);
    assert_eq!(step.velocity, [0.0; 3]);
    assert_eq!(step.blocked, Some(Blocked::Embedded));
}

#[test]
fn corner_bounces_resolve_multiple_contacts_and_capacity_stops_safely() {
    let walls = [voxel([1, 0, 0]), voxel([-2, 0, 0])];
    let initial = state([0.0, 0.5, 0.5], [500.0, 0.0, 0.0]);
    let step = integrate(initial, body(Response::Bounce), 0.02, &walls, limits()).unwrap();
    assert!(step.contacts.len() >= 2);
    assert!(step.position[0] >= -0.75 && step.position[0] <= 0.75);
    let mut bounded = limits();
    bounded.contacts = 1;
    let capped = integrate(initial, body(Response::Bounce), 0.02, &walls, bounded).unwrap();
    assert_eq!(capped.blocked, Some(Blocked::ContactCapacity));
    assert_eq!(capped.position[0], 0.75);
    assert_eq!(capped.velocity, [0.0; 3]);
}

#[test]
fn budgets_and_invalid_inputs_reject_instead_of_truncating() {
    let mut cap = limits();
    cap.colliders = 0;
    assert_eq!(
        integrate(
            state([0.0; 3], [0.0; 3]),
            body(Response::Stop),
            0.02,
            &[voxel([5; 3])],
            cap
        )
        .unwrap_err(),
        Error::ColliderCapacity
    );
    assert_eq!(
        swept_cells([0.0; 3], [100.0; 3], [0.25; 3], 8192),
        Err(Error::SweepCapacity)
    );
    assert_eq!(
        integrate(
            state([f64::NAN; 3], [0.0; 3]),
            body(Response::Stop),
            0.02,
            &[],
            limits()
        )
        .unwrap_err(),
        Error::InvalidInput
    );
    assert_eq!(
        integrate(
            state([999.0, 0.0, 0.0], [100.0, 0.0, 0.0]),
            body(Response::Stop),
            0.02,
            &[],
            limits()
        )
        .unwrap_err(),
        Error::WorldBoundary
    );
}

#[test]
fn acceleration_and_capture_are_deterministic_across_chunk_seams() {
    let mut launch = state([15.5, 1.0, 0.5], [20.0, 0.0, 0.0]);
    launch.acceleration = [10.0, -20.0, 0.0];
    let step = integrate(launch, body(Response::Stop), 0.02, &[], limits()).unwrap();
    near(step.velocity[0], 20.2);
    near(step.position[0], 15.904);
    near(step.position[1], 0.992);
    let (min, max) = swept_cells(launch.position, [0.404, -0.008, 0.0], [0.25; 3], 8192).unwrap();
    assert_eq!((min[0], max[0]), (15, 16));
}

#[test]
fn earlier_wall_contact_prevents_false_world_boundary_removal() {
    let mut bounds = limits();
    bounds.world_max[0] = 10.0;
    let step = integrate_until_contact(
        state([0.0, 0.5, 0.5], [1000.0, 0.0, 0.0]),
        body(Response::Stop),
        0.02,
        &[voxel([5, 0, 0])],
        bounds,
    )
    .unwrap();
    assert_eq!(step.position, [4.75, 0.5, 0.5]);
    assert_eq!(step.blocked, None);
}
