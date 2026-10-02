//! Bounded outdoor openings for listener muffling, independent of rain clipping.
use glam::Vec3;

pub(super) fn exposure(
    eye: Vec3,
    top: i32,
    mut roof_block: impl FnMut(i32, i32, i32) -> Option<bool>,
) -> f32 {
    let y = eye.y.floor() as i32;
    let sky = |x, z, block: &mut _| super::column_cover(x, y, z, top, block) <= eye.y;
    if sky(eye.x.floor() as i32, eye.z.floor() as i32, &mut roof_block) {
        return 1.0;
    }
    // Probe the acoustic patch at eye height. A ray must travel through known
    // open cells to a column with known sky; roofs alone do not seal an entrance.
    // Leaves remain acoustically porous. Unknown chunks never create openings.
    let mut openness = 0.0;
    for direction in 0..16 {
        let angle = direction as f32 * std::f32::consts::TAU / 16.0;
        let (dz, dx) = angle.sin_cos();
        let mut previous = (eye.x.floor() as i32, eye.z.floor() as i32);
        for step in 1..=16 {
            let distance = step as f32 * 0.5;
            let x = (eye.x + dx * distance).floor() as i32;
            let z = (eye.z + dz * distance).floor() as i32;
            if (x, z) == previous {
                continue;
            }
            previous = (x, z);
            if roof_block(x, y, z) != Some(false) {
                break;
            }
            if sky(x, z, &mut roof_block) {
                openness += 1.0 / (1.0 + distance / 4.0);
                break;
            }
        }
    }
    // An entrance spans only part of the listener's surroundings. Retain a
    // sheltered timbre, with enough direct outdoor sound for narrow openings.
    0.8 * (openness / 16.0).sqrt()
}

#[cfg(test)]
mod tests;
