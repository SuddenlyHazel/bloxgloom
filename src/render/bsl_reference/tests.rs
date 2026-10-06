use super::*;

#[test]
fn source_orbit_has_default_tilt_and_wraps_custom_time() {
    let noon = Celestial::from_sun_angle(0.25);
    assert!((noon.time_angle - 0.25).abs() < 1e-6);
    assert!(noon.sun.distance(Vec3::new(0.0, 0.76604444, 0.64278764)) < 1e-6);
    for angle in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9] {
        let a = Celestial::from_sun_angle(angle);
        let b = Celestial::from_sun_angle(angle + 1.0);
        assert!(a.sun.distance(b.sun) < 2e-6);
        assert!((a.sun.length() - 1.0).abs() < 1e-6);
    }
    assert_eq!(shadow_fade(0.25), 1.0);
    assert_eq!(shadow_fade(0.75), 1.0);
    assert_eq!(shadow_fade(0.0), 0.0);
    assert_eq!(shadow_fade(0.5), 0.0);
}

#[test]
fn reference_palettes_match_checked_in_noon_moon_and_rain_defaults() {
    let mut noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    noon.time_angle = 0.25;
    noon.sun = Vec3::Y;
    let (light, ambient) = palettes(noon);
    let expected = Vec3::new(196., 220., 255.) * (1.4 / 255.);
    assert!(light.distance(expected * expected) < 1e-6);
    let expected = Vec3::new(120., 172., 255.) * (0.60 / 255.);
    assert!(ambient.distance(expected * expected) < 1e-6);
    let night = Atmosphere {
        sun: -Vec3::Y,
        time_angle: 0.75,
        moon_phase: 4,
        ..noon
    };
    let (light, ambient) = palettes(night);
    let expected = Vec3::new(96., 192., 255.) * (0.3 * 0.5 / 255.);
    assert!(light.distance(expected * expected) < 1e-7);
    assert!(ambient.distance(expected * expected * 0.36) < 1e-7);
    let (rain, _) = palettes(Atmosphere {
        rain_strength: 1.0,
        ..noon
    });
    let raw = Vec3::new(196., 220., 255.) * (1.4 / 255.);
    let gray = raw.dot(Vec3::new(0.299, 0.587, 0.114));
    let tinted = Vec3::new(176., 224., 255.) * (1.2 / 255.) * gray;
    assert!(rain.distance(tinted * tinted) < 1e-6);
}

#[path = "gpu.rs"]
mod gpu;

#[path = "albedo/tests.rs"]
mod albedo;
