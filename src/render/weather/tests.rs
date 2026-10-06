use super::*;

#[test]
fn full_storm_replaces_day_and_twilight_sky_colors_with_neutral_overcast() {
    let storm = Presentation::new(1.0, 1.0, [0.0; 2], 1.0, 0.0, 0.0);
    for time in [crate::daylight::INITIAL_MS, crate::daylight::CYCLE_MS / 2] {
        let atmosphere = storm.atmosphere(Atmosphere::at(time));
        for color in [atmosphere.horizon, atmosphere.zenith] {
            assert!(color.z >= color.x);
            assert!(
                color.z - color.x < 0.02,
                "storm sky retains a colored clear-sky gradient: {color:?}"
            );
        }
    }
}
fn camera() -> Camera {
    Camera {
        position: Vec3::new(0.0, 10.0, 0.0),
        yaw: 0.0,
        pitch: 0.0,
        fov_y_radians: 1.2,
    }
}
#[test]
fn rain_is_bounded_and_roof_clipping_keeps_streaks_above_cover() {
    let mut weather = Presentation::new(1.0, 1.0, [18.0, 0.0], 1.0, 7.0, 0.0);
    let open = weather.vertices(camera());
    assert_eq!(open.len(), 512 * 27);
    weather.set_cover([-8, -8], [12.0; 256]);
    let covered = weather.vertices(camera());
    assert!(covered.len() < open.len());
    assert!(covered.chunks_exact(9).all(|v| v[1] >= 12.0));
    let mut edge = [f32::NEG_INFINITY; 256];
    for z in 0..16 {
        for x in 8..16 {
            edge[z * 16 + x] = 12.0;
        }
    }
    weather.set_cover([-8, -8], edge);
    assert!(weather.vertices(camera()).chunks_exact(9).all(|v| {
        let x = (v[0].floor() as i32 + 8) as usize;
        let z = (v[2].floor() as i32 + 8) as usize;
        v[1] >= edge[z * 16 + x]
    }));
    weather.set_cover([-8, -8], [f32::INFINITY; 256]);
    assert!(weather.vertices(camera()).is_empty());
}
#[test]
fn clear_and_sheltered_rain_are_empty_and_flash_preserves_sun_direction() {
    assert!(Presentation::default().vertices(camera()).is_empty());
    let hidden = Presentation::new(1.0, 1.0, [0.0; 2], 0.0, 0.0, 1.0);
    assert!(hidden.vertices(camera()).is_empty());
    let night = Atmosphere::at(crate::daylight::CYCLE_MS * 3 / 4);
    let flash = hidden.atmosphere(night);
    assert_eq!(flash.sun, night.sun);
    assert!(flash.strength > night.strength);
}

#[test]
fn cloud_advection_is_continuous_across_wind_changes_and_clock_wrap() {
    let base = Atmosphere::at(crate::daylight::INITIAL_MS);
    let clouds = |seconds, wind| {
        Presentation::new(1.0, 1.0, [wind, 0.0], 1.0, seconds, 0.0)
            .atmosphere(base)
            .drift
    };
    assert_eq!(clouds(600.0, 2.0), clouds(600.0, 18.0));
    let before = clouds(3599.99, 18.0);
    let after = clouds(0.01, 2.0);
    assert!((before[0] - after[0]).abs() < 0.001);
    assert!((before[1] - after[1]).abs() < 0.001);
}

#[test]
fn severe_storm_increases_rain_wind_and_fog_with_bounded_geometry() {
    let base = Atmosphere::at(crate::daylight::INITIAL_MS);
    let profile = |kind: crate::weather::WeatherKind| {
        let values = kind.values();
        Presentation::new(values.cloud, values.rain, [values.wind, 0.0], 1.0, 0.0, 0.0)
    };
    let mild = profile(crate::weather::WeatherKind::StormMild);
    let normal = profile(crate::weather::WeatherKind::Storm);
    let severe = profile(crate::weather::WeatherKind::StormSevere);
    assert!(mild.vertices(camera()).len() < normal.vertices(camera()).len());
    assert!(normal.vertices(camera()).len() < severe.vertices(camera()).len());
    assert!(severe.vertices(camera()).len() <= MAX_STREAKS * 27);
    assert_eq!(severe.wind[0], 30.0);
    assert!(mild.atmosphere(base).fog < normal.atmosphere(base).fog);
    assert!(normal.atmosphere(base).fog < severe.atmosphere(base).fog);
    assert!(
        (severe
            .atmosphere(base)
            .camera_data(glam::Mat4::IDENTITY, glam::Vec3::ZERO)[23]
            - 0.065)
            .abs()
            < 1e-6
    );
    assert_eq!(Presentation::default().atmosphere(base).fog, 0.0);
}

#[test]
fn precipitation_strength_does_not_follow_cloud_coverage_or_reset_moon_phase() {
    let base = Atmosphere::at(crate::daylight::CYCLE_MS * 4 + crate::daylight::INITIAL_MS);
    let dry_overcast = Presentation::new(1.0, 0.0, [0.0; 2], 1.0, 0.0, 0.0).atmosphere(base);
    let rain = Presentation::new(0.2, 0.75, [0.0; 2], 1.0, 0.0, 0.0).atmosphere(base);
    let heavy = Presentation::new(0.2, 1.8, [0.0; 2], 1.0, 0.0, 0.0).atmosphere(base);
    assert_eq!((dry_overcast.cloud, dry_overcast.rain_strength), (1.0, 0.0));
    assert_eq!((rain.cloud, rain.rain_strength), (0.2, 0.75));
    assert_eq!(heavy.rain_strength, 1.0);
    assert_eq!(rain.moon_phase, 4);
    assert_eq!(rain.moon_multiplier(), 0.5);
    assert_eq!(
        Presentation::new(0.0, 0.0, [0.0; 2], 1.0, 3601.25, 0.0)
            .atmosphere(base)
            .presentation_seconds,
        if crate::render::bsl_reference::enabled() {
            3601.25
        } else {
            1.25
        }
    );
    assert_eq!(
        Presentation::new(1.0, f32::NAN, [0.0; 2], 1.0, 0.0, 0.0)
            .atmosphere(base)
            .rain_strength,
        0.0
    );
}
