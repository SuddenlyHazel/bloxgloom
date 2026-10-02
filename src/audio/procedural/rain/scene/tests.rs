use super::*;
use crate::audio::rain_scene::RainMaterial;

#[test]
fn tiles_preserve_material_location_and_turn_with_listener() {
    let mut sampler = SceneSampler {
        position: [0.0, 1.0, 0.0],
        scene: Some(Arc::new(RainScene {
            tiles: vec![
                RainTile {
                    impact: None,
                    centre: [0.0, 1.0, 2.0],
                    material: RainMaterial::Leaf,
                    habitat: crate::audio::rain_scene::Habitat::None,
                    normal: [0.0; 2],
                },
                RainTile {
                    impact: None,
                    centre: [0.0, 1.0, -2.0],
                    material: RainMaterial::Concrete,
                    habitat: crate::audio::rain_scene::Habitat::None,
                    normal: [0.0; 2],
                },
            ],
        })),
        ..Default::default()
    };
    sampler.rebuild(RainWeather::default(), 1.0);
    let tile = sampler.choose(0.25).unwrap();
    assert_eq!(tile.material, RainMaterial::Leaf);
    let (distance, angle) = sampler.polar(tile, 0.5, 0.5);
    assert_eq!(distance, 2.0);
    assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    sampler.yaw = std::f32::consts::PI;
    let (_, turned) = sampler.polar(tile, 0.5, 0.5);
    assert!((turned - 3.0 * std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert_eq!(
        sampler.choose(0.75).unwrap().material,
        RainMaterial::Concrete
    );
}

#[test]
fn walls_only_receive_windward_driving_rain_and_empty_scenes_are_silent() {
    let tile = RainTile {
        impact: None,
        centre: [2.0, 0.0, 0.0],
        material: RainMaterial::Metal,
        habitat: crate::audio::rain_scene::Habitat::None,
        normal: [-1.0, 0.0],
    };
    let mut sampler = SceneSampler {
        scene: Some(Arc::new(RainScene { tiles: vec![tile] })),
        ..Default::default()
    };
    sampler.rebuild(RainWeather::default(), 0.0);
    assert!(sampler.choose(0.0).is_none());
    sampler.rebuild(RainWeather::default(), 2.0);
    assert_eq!(sampler.area, 2.0);
    assert_eq!(sampler.choose(0.0), Some(tile));
    sampler.rebuild(
        RainWeather {
            wind_bearing_rad: std::f32::consts::PI,
            ..RainWeather::default()
        },
        2.0,
    );
    assert!(sampler.choose(0.0).is_none());
    sampler.scene = Some(Arc::new(RainScene::default()));
    sampler.rebuild(RainWeather::default(), 1.0);
    assert!(sampler.choose(0.5).is_none());
}

#[test]
fn listener_turning_does_not_change_world_wall_exposure() {
    let mut sampler = SceneSampler {
        scene: Some(Arc::new(RainScene {
            tiles: vec![RainTile {
                impact: None,
                centre: [2.0, 0.0, 0.0],
                material: RainMaterial::Metal,
                habitat: crate::audio::rain_scene::Habitat::None,
                normal: [-1.0, 0.0],
            }],
        })),
        ..Default::default()
    };
    sampler.rebuild(
        RainWeather {
            wind_bearing_rad: 0.35,
            ..RainWeather::default()
        },
        2.0,
    );
    let area = sampler.area;
    sampler.yaw = 1.0;
    sampler.rebuild(
        RainWeather {
            wind_bearing_rad: 0.35 - 1.0,
            ..RainWeather::default()
        },
        2.0,
    );
    assert!((sampler.area - area).abs() < 1e-5);
}
