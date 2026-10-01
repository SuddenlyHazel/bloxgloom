use super::*;
use crate::audio::rain_scene::{RainMaterial, RainScene};

fn energy(engine: &mut Insects, seconds: usize) -> f64 {
    let mut energy = 0.0;
    for _ in 0..seconds * 44_100 {
        let (sample, send) = engine.next();
        assert!(sample.into_iter().all(f32::is_finite) && send.is_finite());
        energy += sample
            .into_iter()
            .map(|v| f64::from(v).powi(2))
            .sum::<f64>();
    }
    energy
}
#[test]
fn insects_follow_habitat_daylight_and_rain_without_fake_sources() {
    let mut engine = Insects::new(1);
    let night = WeatherSound {
        exposure: 1.0,
        ..WeatherSound::default()
    };
    engine.follow(Some(night), [0.0, 1.6, 0.0], 0.0);
    assert_eq!(energy(&mut engine, 1), 0.0);
    engine.set_scene(&RainScene::patch(RainMaterial::Dirt));
    engine.follow(Some(night), [0.0, 1.6, 0.0], 0.0);
    assert_eq!(engine.allowed, [true, false]);
    assert!(energy(&mut engine, 3) > 1e-4);
    let day = WeatherSound {
        daylight: 1.0,
        ..night
    };
    engine.set_scene(&RainScene::patch(RainMaterial::Leaf));
    engine.follow(Some(day), [0.0, 1.6, 0.0], 0.0);
    assert_eq!(engine.allowed, [false, true]);
    assert!(energy(&mut engine, 3) > 1e-4);
    engine.follow(
        Some(WeatherSound {
            rain_mm_h: 10.0,
            ..day
        }),
        [0.0, 1.6, 0.0],
        0.0,
    );
    assert_eq!(engine.allowed, [false, false]);
    energy(&mut engine, 8);
    assert!(energy(&mut engine, 1) < 1e-10);
}
#[test]
fn habitat_sources_are_stable_under_scene_order_and_listener_movement() {
    let mut scene = (*RainScene::patch(RainMaterial::Leaf)).clone();
    let original = sources(&scene, Habitat::Canopy, 42);
    scene.tiles.reverse();
    assert_eq!(sources(&scene, Habitat::Canopy, 42), original);
    assert_eq!(sources(&scene, Habitat::Ground, 42), [None; VOICES]);
    let mut a = Insects::new(42);
    let mut b = Insects::new(42);
    a.set_scene(&scene);
    b.set_scene(&scene);
    let weather = Some(WeatherSound {
        daylight: 1.0,
        exposure: 1.0,
        ..WeatherSound::default()
    });
    a.follow(weather, [0.0, 1.6, 0.0], 0.0);
    b.follow(weather, [0.0, 1.6, 0.0], 0.0);
    for _ in 0..10_000 {
        assert_eq!(a.next(), b.next());
    }
    b.follow(weather, [3.0, 1.6, 0.0], 1.0);
    assert_eq!(a.cicada_sources, b.cicada_sources);
    let mut difference = 0.0;
    for _ in 0..10_000 {
        let x = a.next().0;
        let y = b.next().0;
        difference += (x[0] - y[0]).abs() + (x[1] - y[1]).abs();
    }
    assert!(
        difference > 0.01,
        "moving/turning must affect the spatial profile"
    );
    b.set_scene(&RainScene::default());
    b.follow(weather, [3.0, 1.6, 0.0], 1.0);
    assert_eq!(b.cicada_sources, [None; VOICES]);
    energy(&mut b, 8);
    assert!(energy(&mut b, 1) < 1e-10);
}
