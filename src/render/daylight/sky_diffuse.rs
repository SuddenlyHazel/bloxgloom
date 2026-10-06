//! Base-sky diffuse radiance, convolved with a unit Lambertian receiver.
//!
//! The generated table samples the actual BSL HDR sky and ground mask, before
//! clouds, scene occlusion, solar NEE or presentation. Runtime interpolation is
//! bounded and allocation-free. Lunar dependence is exactly quadratic in the
//! source equations; solar elevation and rain use piecewise linear interpolation.
//! Up/down endpoints retain an explicit fallback in the source units. Enhanced
//! surfaces use compact real SH9 from the angular submodule, retaining sun azimuth.
use glam::Vec3;
mod angular;

pub(super) fn angular_coefficients(sun_y: f32, rain: f32, moon: f32) -> [Vec3; 6] {
    angular::coefficients(sun_y, rain, moon)
}

const DATA: &[u8] = include_bytes!("sky_diffuse.bin");
const RAIN_LEVELS: usize = 9;
const COMPONENTS: usize = 6;
const COEFFICIENTS: usize = 3;

fn word(index: usize) -> [u8; 4] {
    DATA[index * 4..index * 4 + 4].try_into().unwrap()
}

fn scalar(index: usize) -> f32 {
    f32::from_le_bytes(word(index))
}

fn elevation(index: usize) -> f32 {
    scalar(1 + index)
}

fn sample(sun: usize, rain: usize, moon: f32) -> [f32; COMPONENTS] {
    let count = u32::from_le_bytes(word(0)) as usize;
    let start = 1 + count + (sun * RAIN_LEVELS + rain) * COMPONENTS * COEFFICIENTS;
    std::array::from_fn(|component| {
        scalar(start + component)
            + moon * scalar(start + COMPONENTS + component)
            + moon * moon * scalar(start + COMPONENTS * 2 + component)
    })
}

pub(super) fn convolved(sun_y: f32, rain: f32, moon: f32) -> (Vec3, Vec3) {
    let count = u32::from_le_bytes(word(0)) as usize;
    let y = sun_y.clamp(elevation(0), elevation(count - 1));
    let (mut lower, mut upper) = (0, count - 1);
    while upper - lower > 1 {
        let middle = (lower + upper) / 2;
        if elevation(middle) <= y {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    let solar_weight = (y - elevation(lower)) / (elevation(upper) - elevation(lower));
    let rain_position = rain.clamp(0.0, 1.0) * (RAIN_LEVELS - 1) as f32;
    let rain_lower = (rain_position.floor() as usize).min(RAIN_LEVELS - 2);
    let rain_weight = rain_position - rain_lower as f32;
    let moon = moon.clamp(0.0, 1.0);
    let a = sample(lower, rain_lower, moon);
    let b = sample(lower, rain_lower + 1, moon);
    let c = sample(upper, rain_lower, moon);
    let d = sample(upper, rain_lower + 1, moon);
    let values: [f32; COMPONENTS] = std::array::from_fn(|i| {
        let low = a[i] + (b[i] - a[i]) * rain_weight;
        let high = c[i] + (d[i] - c[i]) * rain_weight;
        (low + (high - low) * solar_weight).max(0.0)
    });
    (
        Vec3::new(values[3], values[4], values[5]),
        Vec3::new(values[0], values[1], values[2]),
    )
}
