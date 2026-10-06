//! Six-term representation of cosine-convolved real SH9. Reflection symmetry
//! about the sun/up plane removes three odd-Z terms. Canonical X points away
//! from the horizontal sun; runtime evaluates signed RGB polynomial coefficients.
use glam::Vec3;
const DATA: &[u8] = include_bytes!("angular.bin");
const RAIN: usize = 9;
const COMPONENTS: usize = 18;
fn word(index: usize) -> [u8; 4] {
    DATA[index * 4..index * 4 + 4].try_into().unwrap()
}
fn scalar(index: usize) -> f32 {
    f32::from_le_bytes(word(index))
}
fn sample(sun: usize, rain: usize, moon: f32) -> [f32; COMPONENTS] {
    let count = u32::from_le_bytes(word(0)) as usize;
    let start = 1 + count + (sun * RAIN + rain) * COMPONENTS * 3;
    std::array::from_fn(|i| {
        scalar(start + i)
            + moon * scalar(start + COMPONENTS + i)
            + moon * moon * scalar(start + COMPONENTS * 2 + i)
    })
}
pub(super) fn coefficients(sun_y: f32, rain: f32, moon: f32) -> [Vec3; 6] {
    let count = u32::from_le_bytes(word(0)) as usize;
    let y = sun_y.clamp(scalar(1), scalar(count));
    let (mut lower, mut upper) = (0, count - 1);
    while upper - lower > 1 {
        let middle = (lower + upper) / 2;
        if scalar(1 + middle) <= y {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    let sy = (y - scalar(1 + lower)) / (scalar(1 + upper) - scalar(1 + lower));
    let r = rain.clamp(0.0, 1.0) * (RAIN - 1) as f32;
    let r0 = (r.floor() as usize).min(RAIN - 2);
    let ry = r - r0 as f32;
    let m = moon.clamp(0.0, 1.0);
    let a = sample(lower, r0, m);
    let b = sample(lower, r0 + 1, m);
    let c = sample(upper, r0, m);
    let d = sample(upper, r0 + 1, m);
    let values: [f32; COMPONENTS] = std::array::from_fn(|i| {
        (a[i] + (b[i] - a[i]) * ry) * (1.0 - sy) + (c[i] + (d[i] - c[i]) * ry) * sy
    });
    std::array::from_fn(|i| Vec3::from_array(values[i * 3..i * 3 + 3].try_into().unwrap()))
}
