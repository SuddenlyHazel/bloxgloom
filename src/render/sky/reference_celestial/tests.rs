use super::SHADER;
use wgpu::naga;
#[path = "gpu.rs"]
mod gpu;

// Independent translation of the checked-in default GLSL, intentionally using
// its intermediate names and output sqrt followed by the deferred square.
const ORACLE: &str = include_str!("oracle.wgsl");

#[test]
fn reference_celestial_source_parses_and_preserves_source_settings() {
    let source = format!("{SHADER}\n{ORACLE}");
    let module = naga::front::wgsl::parse_str(&source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    if let Some(settings) = crate::render::bsl_reference::audit_source("lib/settings.glsl") {
        for expected in [
            "#define SUN_INTENSITY 1.50",
            "#define MOON_INTENSITY 1.50",
            "#define SUN_MOON_GROUND",
            "#define SKY_DESATURATION",
            "#define NIGHT_MOON_PHASE",
        ] {
            assert!(
                settings.contains(expected),
                "source default changed: {expected}"
            );
        }
    }
}

fn source_cpu(sampled: [f32; 4], up: f32, moon: bool, visibility: f32, phase: f32) -> [f64; 3] {
    let base = 1.0 - (1.0 - (f64::from(up) * 0.975 + 0.025).max(0.0)).powi(8);
    let t = base.clamp(0.0, 1.0);
    let fade = t * t * (3.0 - 2.0 * t);
    let rgb = std::array::from_fn::<_, 3, _>(|i| {
        f64::from(sampled[i]).powf(2.2) * f64::from(sampled[3]) * 2.25 * fade * fade
    });
    if !moon {
        return rgb;
    }
    let luminance = rgb
        .iter()
        .zip([0.299, 0.587, 0.114])
        .map(|(v, w)| v * w)
        .sum::<f64>();
    std::array::from_fn(|i| {
        let desat =
            luminance * ([96.0, 192.0, 255.0][i] * 0.3 * f64::from(phase) / 255.0).powf(1.6) * 4.0;
        desat * (1.0 - f64::from(visibility)) + rgb[i] * f64::from(visibility)
    })
}
