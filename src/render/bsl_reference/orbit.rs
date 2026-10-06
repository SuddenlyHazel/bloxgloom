//! shaders.properties custom time uniforms plus gbuffers_terrain.glsl orbit.
use glam::Vec3;

pub(crate) struct Celestial {
    pub sun: Vec3,
    pub time_angle: f32,
}
impl Celestial {
    /// sunAngle has Iris's normalized sunrise/noon/sunset/midnight convention.
    /// The engine supplies its authoritative normalized day phase; a matched
    /// Minecraft getTimeOfDay/tickDelta input is still a separate runtime input.
    pub(crate) fn from_sun_angle(sun_angle: f32) -> Self {
        let sun_angle = f64::from(sun_angle);
        let minimum = (sun_angle - 0.033333333).rem_euclid(1.0);
        let linear = if minimum < 0.433333333 {
            minimum * 1.15384615385
        } else {
            minimum * 0.882352941176 + 0.117647058824
        };
        let half = f64::from(linear > 0.5);
        let fraction = (linear * 2.0).fract();
        let smooth = fraction * fraction * (3.0 - 2.0 * fraction);
        let mixing = if half < 0.5 { 0.3 } else { -0.1 };
        let time_angle = (fraction * (1.0 - mixing) + smooth * mixing + half) * 0.5;
        let phase = (time_angle - 0.25).rem_euclid(1.0);
        let angle = (phase + ((phase * std::f64::consts::PI).cos() * -0.5 + 0.5 - phase) / 3.0)
            * std::f64::consts::TAU;
        let rotation = -40.0f64.to_radians();
        let sun = Vec3::new(
            -angle.sin() as f32,
            (angle.cos() * rotation.cos()) as f32,
            (-angle.cos() * rotation.sin()) as f32,
        )
        .normalize();
        Self {
            sun,
            time_angle: time_angle as f32,
        }
    }
}

pub(crate) fn shadow_fade(sun_angle: f32) -> f32 {
    (1.0 - (((sun_angle - 0.5).abs() - 0.25).abs() - 0.23) * 100.0).clamp(0.0, 1.0)
}
