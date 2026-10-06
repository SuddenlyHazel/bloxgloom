//! Atmosphere uniforms shared by terrain, actors, fog, and the procedural sky.
use super::SUN_DIRECTION;
use glam::{Mat4, Vec3};

/// Compose calibrated lighting and fog without reserving shadow bindings.
/// Distant terrain shares this basis but owns a separate coverage bind group.
pub(super) fn surface_shader(source: &str) -> String {
    super::fog::shader(&format!(
        "{}\n{}\n{}\n{}\n{}\n{source}",
        super::bsl_reference::shader(),
        super::sky::CLOUD_SHADER,
        super::bsl_reference::surface_shader(),
        include_str!("daylight/primary_cloud.wgsl"),
        include_str!("scene_ao_output.wgsl")
    ))
}

/// Near opaque surfaces additionally sample the camera-local sun shadow map.
pub(super) fn shader(source: &str) -> String {
    surface_shader(&format!(
        "{}\n{}\n{}\n{source}",
        include_str!("local_shadow.wgsl"),
        super::sun_shadow::SHADER,
        include_str!("scene_contact.wgsl")
    ))
}

#[derive(Clone, Copy)]
pub(crate) struct Atmosphere {
    pub sun: Vec3,
    pub sun_color: Vec3,
    pub lighting: crate::config::lighting::Lighting,
    pub strength: f32,
    pub horizon: Vec3,
    pub zenith: Vec3,
    pub cloud: f32,
    /// Precipitation strength, independent of cloud coverage (BSL rainStrength).
    pub rain_strength: f32,
    /// Server-clock day index modulo the eight-phase BSL lunar cycle.
    pub moon_phase: u32,
    /// Bounded presentation clock for BSL frame-time sky animation.
    pub presentation_seconds: f32,
    pub fog: f32,
    pub fog_exposure: f32,
    pub drift: [f32; 2],
    /// Shared by foliage receivers and all depth casters. The 128-second
    /// period is exact for the integer-frequency wind harmonics.
    pub wind_seconds: f32,
    /// Scene-space transport owns camera extinction/scattering when available.
    pub scene_transport: bool,
    /// BSL's custom timeAngle, kept separate from its warped celestial vector.
    pub time_angle: f32,
    pub reference_shadow_fade: f32,
}

impl Atmosphere {
    pub(crate) fn at(time: u64) -> Self {
        let phase = crate::daylight::phase(time);
        let celestial = super::bsl_reference::Celestial::from_sun_angle(phase);
        let angle = phase * std::f32::consts::TAU;
        let midday = SUN_DIRECTION.normalize();
        let tangent = Vec3::new(midday.z, 0.0, -midday.x).normalize();
        let sun = if super::bsl_reference::enabled() {
            celestial.sun
        } else {
            (midday * angle.sin() + tangent * angle.cos()).normalize()
        };
        let day = smooth(-0.12, 0.18, sun.y);
        let twilight = (1.0 - (sun.y / 0.22).abs()).max(0.0) * day;
        Self {
            sun,
            sun_color: Vec3::new(0.90, 0.30, 0.10)
                .lerp(Vec3::new(0.72, 0.67, 0.56), smooth(0.0, 0.40, sun.y)),
            lighting: Default::default(),
            cloud: 0.0,
            rain_strength: 0.0,
            moon_phase: ((time / crate::daylight::CYCLE_MS) % 8) as u32,
            presentation_seconds: 0.0,
            fog: 0.0,
            fog_exposure: 1.0,
            drift: [0.0; 2],
            wind_seconds: (time % 128_000) as f32 / 1_000.0,
            scene_transport: false,
            time_angle: celestial.time_angle,
            reference_shadow_fade: super::bsl_reference::shadow_fade(phase),
            strength: 0.035 + 0.965 * day,
            horizon: Vec3::new(0.012, 0.018, 0.045)
                .lerp(Vec3::new(0.59, 0.72, 0.82), day)
                .lerp(Vec3::new(0.80, 0.30, 0.14), twilight * 0.55),
            zenith: Vec3::new(0.002, 0.005, 0.020).lerp(Vec3::new(0.20, 0.45, 0.75), day),
        }
    }

    /// BSL NIGHT_MOON_PHASE default table, indexed by authoritative world time.
    pub(crate) fn moon_multiplier(self) -> f32 {
        [1.0, 0.875, 0.75, 0.625, 0.5, 0.625, 0.75, 0.875][self.moon_phase as usize % 8]
    }

    /// Directional irradiance for diffuse/GGX receivers (historical API name).
    /// The visible solar disc applies a separate artistic display scale.
    pub(crate) fn sun_radiance(self) -> Vec3 {
        if super::bsl_reference::enabled() {
            return super::bsl_reference::palettes(self).0;
        }
        self.sun_color
            * self.strength
            * smooth(-0.02, 0.12, self.sun.y)
            * if self.scene_transport {
                1.0
            } else {
                (1.0 - self.cloud).powi(2)
            }
            * self.lighting.sun_intensity
            * 2.4
    }

    pub(crate) fn ambient(self) -> (Vec3, Vec3) {
        if super::bsl_reference::enabled() {
            let color = super::bsl_reference::palettes(self).1;
            return (color, color);
        }
        // A broad hemispherical convolution desaturates the sky. Calibrate its
        // clear-noon energy once; do not adapt exposure as the scene darkens.
        fn convolve(color: Vec3, reference: Vec3, energy: Vec3) -> Vec3 {
            let weights = Vec3::new(0.2126, 0.7152, 0.0722);
            let luminance = color.dot(weights) / reference.dot(weights);
            energy * Vec3::splat(luminance).lerp(color / reference, 0.35)
        }
        let horizon = Vec3::new(0.59, 0.72, 0.82);
        let zenith = Vec3::new(0.20, 0.45, 0.75);
        let lower = convolve(self.horizon, horizon, Vec3::new(0.36, 0.335, 0.30));
        let upper = convolve(
            self.zenith.lerp(self.horizon, 0.35),
            zenith.lerp(horizon, 0.35),
            Vec3::new(0.55, 0.57, 0.60),
        );
        (
            lower * self.lighting.ambient_intensity,
            upper * self.lighting.ambient_intensity,
        )
    }

    pub(crate) fn time_brightness(self) -> f32 {
        if super::bsl_reference::enabled() {
            (self.time_angle * std::f32::consts::TAU).sin().max(0.0)
        } else {
            (self.sun.y / SUN_DIRECTION.normalize().y).clamp(0.0, 1.0)
        }
    }

    pub(crate) fn light_direction(self) -> Vec3 {
        if super::bsl_reference::enabled() && self.time_angle >= 0.5325 && self.time_angle <= 0.9675
        {
            -self.sun
        } else {
            self.sun
        }
    }

    pub(crate) fn camera_data(self, matrix: Mat4, eye: Vec3) -> [f32; 56] {
        let mut data = [0.0; 56];
        data[..16].copy_from_slice(&matrix.to_cols_array());
        let direction = self.light_direction();
        data[16..20].copy_from_slice(&[
            direction.x,
            direction.y,
            direction.z,
            if super::bsl_reference::enabled() {
                self.time_brightness()
            } else {
                self.strength
            },
        ]);
        data[20..23].copy_from_slice(&self.horizon.to_array());
        data[23] = super::fog::density(self.fog, 1.0);
        data[24..27].copy_from_slice(&eye.to_array());
        data[27] = self.fog_exposure;
        if super::bsl_reference::enabled() {
            // Reference Overworld has no vanilla far fade. Reuse these slots
            // for explicit source eye altitude / engine bedrock instead.
            data[28] = eye.y;
            data[29] = crate::world::BEDROCK_Y as f32;
        } else {
            data[28] = 38.0;
            data[29] = 135.0;
        }
        data[30] = self.wind_seconds;
        data[31] = f32::from(self.scene_transport);
        // Preserve the terrain parallax slot at byte 128 for every camera.
        data[32..36].copy_from_slice(&crate::config::parallax::Parallax::default().uniform());
        data[36..39].copy_from_slice(&self.sun_radiance().to_array());
        // Shared sky/fog styling uses actual rain and lunar phase, independent
        // of the brighter horizon colors calibrated for indirect illumination.
        data[39] = self.rain_strength;
        data[40..43].copy_from_slice(&self.zenith.to_array());
        data[43] = self.lighting.environment_intensity;
        let (lower, upper) = self.ambient();
        data[44..47].copy_from_slice(&lower.to_array());
        data[47] = (0.65 * self.lighting.local_directionality).clamp(0.0, 1.0);
        data[48..51].copy_from_slice(&upper.to_array());
        data[51] = self.moon_multiplier();
        if super::bsl_reference::enabled() {
            data[43] = self.moon_multiplier();
            data[47] = (self.sun.y * 10.0 + 0.5).clamp(0.0, 1.0);
            data[51] = self.reference_shadow_fade;
            // Only the checked-in ADVANCED-off profile disables relief.
            // The separately named artistic comparison retains engine relief.
            if super::bsl_reference::default_materials() {
                data[32] = 0.0;
            }
        }
        data[52..56].copy_from_slice(&[
            self.cloud,
            self.drift[0],
            self.drift[1],
            f32::from(self.scene_transport),
        ]);
        data
    }
}

fn smooth(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
#[path = "daylight/tests.rs"]
mod tests;
