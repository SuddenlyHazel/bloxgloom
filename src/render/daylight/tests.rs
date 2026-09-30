use super::*;

#[test]
fn sun_and_atmosphere_wrap_and_night_preserves_a_small_sky_light() {
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let midnight = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let wrapped = Atmosphere::at(crate::daylight::INITIAL_MS + crate::daylight::CYCLE_MS);
    assert!(noon.sun.y > 0.6);
    assert!(midnight.sun.y < -0.6);
    assert!((noon.strength - 1.0).abs() < 1e-6);
    assert!((midnight.strength - 0.035).abs() < 1e-6);
    assert_eq!(noon.sun, wrapped.sun);
    assert!(midnight.horizon.length() < noon.horizon.length() / 10.0);
    let dawn = Atmosphere::at(0);
    let before = Atmosphere::at(crate::daylight::CYCLE_MS - 1);
    assert!(dawn.sun.distance(before.sun) < 0.0001);
    assert!(dawn.horizon.distance(before.horizon) < 0.0001);
}
