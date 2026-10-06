use super::super::{Camera, daylight::Atmosphere};
use glam::Vec3;

pub(crate) fn sky_camera_data(
    camera: Camera,
    width: u32,
    height: u32,
    atmosphere: Atmosphere,
) -> [f32; 40] {
    sky_camera_data_at_sample(camera, width, height, atmosphere, 0)
}

/// Explicit submitted sample index for source TAA cloud dithering.
pub(crate) fn sky_camera_data_at_sample(
    camera: Camera,
    width: u32,
    height: u32,
    atmosphere: Atmosphere,
    sample: u32,
) -> [f32; 40] {
    let forward = camera.direction();
    let right = Vec3::new(-camera.yaw.sin(), 0.0, camera.yaw.cos());
    let up = right.cross(forward).normalize();
    let vertical = (camera.fov_y_radians * 0.5).tan();
    let horizontal = vertical * width as f32 / height.max(1) as f32;
    let solar = Atmosphere {
        cloud: 0.0,
        ..atmosphere
    }
    .sun_radiance();
    [
        forward.x,
        forward.y,
        forward.z,
        atmosphere.cloud,
        right.x,
        right.y,
        right.z,
        horizontal,
        up.x,
        up.y,
        up.z,
        vertical,
        atmosphere.sun.x,
        atmosphere.sun.y,
        atmosphere.sun.z,
        f32::from(atmosphere.scene_transport),
        atmosphere.horizon.x,
        atmosphere.horizon.y,
        atmosphere.horizon.z,
        atmosphere.drift[0],
        atmosphere.zenith.x,
        atmosphere.zenith.y,
        atmosphere.zenith.z,
        atmosphere.drift[1],
        solar.x,
        solar.y,
        solar.z,
        atmosphere.time_brightness(),
        camera.position.x,
        camera.position.y,
        camera.position.z,
        f32::from(super::style_enabled()),
        atmosphere.rain_strength,
        atmosphere.moon_multiplier(),
        atmosphere.moon_phase as f32,
        atmosphere.presentation_seconds,
        atmosphere.reference_shadow_fade,
        1.0, // source FOG_DENSITY and all default biome multipliers are 1
        sample as f32,
        if super::super::bsl_reference::enabled() {
            if (0.5325..=0.9675).contains(&atmosphere.time_angle) {
                -1.0
            } else {
                1.0
            }
        } else {
            0.0
        },
    ]
}
