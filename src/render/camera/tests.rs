use super::*;

fn eye() -> Camera {
    Camera {
        position: Vec3::new(0.5, 2.5, 0.5),
        yaw: 0.0,
        pitch: 0.0,
        fov_y_radians: 1.2,
    }
}

#[test]
fn perspectives_orbit_the_eye_and_cycle_without_changing_aim() {
    let eye = eye();
    let first =
        Perspective::FirstPerson.view(eye, |_| panic!("no collision query in first person"));
    assert_eq!(first.position, eye.position);
    let rear = Perspective::Behind.view(eye, |_| false);
    let front = Perspective::Front.view(eye, |_| false);
    assert_eq!(rear.position, eye.position - Vec3::X * DISTANCE);
    assert_eq!(front.position, eye.position + Vec3::X * DISTANCE);
    assert!(front.direction().dot(eye.direction()) < -0.999);
    assert_eq!(
        Perspective::FirstPerson.next().next().next(),
        Perspective::FirstPerson
    );
}

#[test]
fn swept_camera_stops_before_walls_and_handles_close_or_unknown_cells() {
    let rear = Perspective::Behind.view(eye(), |cell| cell[0] == -2);
    assert!((rear.position.x + 0.79).abs() < 0.001);
    let front = Perspective::Front.view(eye(), |cell| cell[0] == 2);
    assert!((front.position.x - 1.79).abs() < 0.001);
    let closed = Perspective::Behind.view(eye(), |_| true);
    assert_eq!(closed.position, eye().position);
    // Center ray clears y=3; the camera's radius still collides with its edge.
    let grazing = Camera {
        position: Vec3::new(0.5, 2.9, 0.5),
        ..eye()
    };
    let rear = Perspective::Behind.view(grazing, |cell| cell[0] == -2 && cell[1] == 3);
    assert!((rear.position.x + 0.79).abs() < 0.001);
}
