//! Opt-in, source-audited BSL default material/lighting reference.
//! This is a comparison path, not a whole-frame Minecraft/Iris parity claim.
mod orbit;
use super::daylight::Atmosphere;
use glam::Vec3;
pub(crate) use orbit::{Celestial, shadow_fade};

pub(crate) const ALBEDO_SHADER: &str = include_str!("bsl_reference/albedo.wgsl");

pub(crate) const LIGHTING_SHADER: &str = include_str!("bsl_reference/lighting.wgsl");
pub(crate) const REFLECTION_SHADER: &str = include_str!("bsl_reference/reflection.wgsl");

pub(crate) fn enabled() -> bool {
    default_materials() || advanced_materials()
}

/// The checked-in ADVANCED_MATERIALS-off profile.
pub(crate) fn default_materials() -> bool {
    std::env::var("BLOXGLOOM_BSL_REFERENCE").is_ok_and(|value| value == "1")
}

/// Explicit artistic material comparison; not the checked-in default profile.
pub(crate) fn advanced_materials() -> bool {
    std::env::var("BLOXGLOOM_BSL_REFERENCE").is_ok_and(|value| value == "advanced")
}

pub(crate) fn shader() -> String {
    format!(
        "const BG_BSL_REFERENCE: bool = {};\nconst BG_BSL_ADVANCED_REFERENCE: bool = {};\n{ALBEDO_SHADER}\n{LIGHTING_SHADER}\n{REFLECTION_SHADER}\n{}",
        default_materials(),
        advanced_materials(),
        include_str!("bsl_reference/camera.wgsl")
    )
}

pub(crate) fn surface_shader() -> String {
    surface_shader_for(enabled())
}

fn surface_shader_for(reference: bool) -> String {
    let source = include_str!("daylight.wgsl");
    if reference {
        format!(
            "{}\n{}",
            source
                .replace("fn bg_surface_light(", "fn bg_enhanced_surface_light(")
                .replace("fn bg_direct_light(", "fn bg_enhanced_direct_light("),
            include_str!("bsl_reference/surface.wgsl")
        )
    } else {
        source.to_owned()
    }
}

/// Exact checked-in lightColor.glsl defaults. Morning and evening palettes
/// coincide in that profile, so its mefade does not change these quantities.
pub(crate) fn palettes(atmosphere: Atmosphere) -> (Vec3, Vec3) {
    let brightness = (atmosphere.time_angle * std::f32::consts::TAU)
        .sin()
        .max(0.0);
    let fade = 1.0 - (1.0 - brightness).powf(1.5);
    let sun = Vec3::new(255.0, 160.0, 80.0)
        .lerp(Vec3::new(196.0, 220.0, 255.0) * (1.4 / 1.2), fade)
        * (1.2 / 255.0);
    let ambient = Vec3::new(255.0, 204.0, 144.0)
        .lerp(Vec3::new(120.0, 172.0, 255.0) * (0.60 / 0.35), fade)
        * (0.35 / 255.0);
    let night = Vec3::new(96.0, 192.0, 255.0) * (0.3 * atmosphere.moon_multiplier() / 255.0);
    let visible = (atmosphere.sun.y * 10.0 + 0.5).clamp(0.0, 1.0);
    let weather = Vec3::new(176.0, 224.0, 255.0) * (1.2 / 255.0);
    let color = |day: Vec3, night: Vec3| {
        let raw = night.lerp(day, visible);
        let gray = raw.dot(Vec3::new(0.299, 0.587, 0.114));
        let tinted = raw.lerp(weather * gray, atmosphere.rain_strength);
        tinted * tinted
    };
    (color(sun, night), color(ambient, night * 0.60))
}

#[cfg(test)]
mod tests;
