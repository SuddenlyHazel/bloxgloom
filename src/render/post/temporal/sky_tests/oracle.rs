use crate::render::Camera;
use glam::{Vec2, Vec3};
fn basis(camera: Camera) -> (Vec3, Vec3, Vec3) {
    let forward = camera.direction();
    let right = Vec3::new(-camera.yaw.sin(), 0., camera.yaw.cos());
    let up = right.cross(forward);
    (forward, right, up)
}
pub(super) fn ray(camera: Camera, uv: Vec2) -> Vec3 {
    let (forward, right, up) = basis(camera);
    let vertical = (camera.fov_y_radians * 0.5).tan();
    (forward + right * (uv.x * 2. - 1.) * (24. / 16.) * vertical + up * (1. - uv.y * 2.) * vertical)
        .normalize()
}
pub(super) fn previous_uv(current: Camera, previous: Camera, uv: Vec2) -> Vec2 {
    let direction = ray(current, uv);
    let (forward, right, up) = basis(previous);
    let scale = (previous.fov_y_radians * 0.5).tan();
    Vec2::new(
        direction.dot(right) / (direction.dot(forward) * scale * 24. / 16.) * 0.5 + 0.5,
        -direction.dot(up) / (direction.dot(forward) * scale) * 0.5 + 0.5,
    )
}
fn bayer(pixel: Vec2, frame: u32) -> f64 {
    let b = |scale: f64| {
        ((f64::from(pixel.x) * scale).floor() * 0.5 + (f64::from(pixel.y) * scale).floor() * 0.75)
            .rem_euclid(1.)
    };
    (b(1.) + b(0.5) * 0.25 + b(0.25) * 0.0625 + f64::from(frame) * 0.618).rem_euclid(1.)
}
pub(super) fn color(camera: Camera, pixel: Vec2, frame: u32) -> [f64; 3] {
    let direction = ray(camera, pixel / Vec2::new(24., 16.));
    let gray = 0.45 + f64::from(direction.y) * 0.12 + f64::from(direction.z) * 0.08;
    let noise = (bayer(Vec2::new(pixel.x, 16. - pixel.y), frame) - 0.5) * 0.20;
    [
        gray + noise,
        gray * 0.8 + noise * 0.35,
        gray * 1.2 - noise * 0.2,
    ]
}
fn ycocg(c: [f64; 3]) -> [f64; 3] {
    [
        (c[0] + 2. * c[1] + c[2]) / 4.,
        (c[0] - c[2]) / 2.,
        (-c[0] + 2. * c[1] - c[2]) / 4.,
    ]
}
pub(super) fn expected(current: Camera, previous: Camera, frame: u32, pixel: Vec2) -> [f64; 3] {
    let uv = pixel / Vec2::new(24., 16.);
    let old_uv = previous_uv(current, previous, uv);
    let p = old_uv * Vec2::new(24., 16.) - Vec2::splat(0.5);
    let base = p.floor();
    let f = p - base;
    let mut history = [0.; 3];
    for y in 0..2 {
        for x in 0..2 {
            let weight = f64::from(if x == 0 { 1. - f.x } else { f.x })
                * f64::from(if y == 0 { 1. - f.y } else { f.y });
            let c = color(
                previous,
                base + Vec2::new(x as f32 + 0.5, y as f32 + 0.5),
                16,
            );
            for i in 0..3 {
                history[i] += c[i] * weight;
            }
        }
    }
    let now = color(current, pixel, frame);
    let mut lo = ycocg(now);
    let mut hi = lo;
    for y in -1..=1 {
        for x in -1..=1 {
            let c = ycocg(color(current, pixel + Vec2::new(x as f32, y as f32), frame));
            for i in 0..3 {
                lo[i] = lo[i].min(c[i]);
                hi[i] = hi[i].max(c[i]);
            }
        }
    }
    let value = ycocg(history);
    let center = std::array::from_fn::<_, 3, _>(|i| (lo[i] + hi[i]) * 0.5);
    let extent = std::array::from_fn::<_, 3, _>(|i| (hi[i] - lo[i]) * 0.5 + 1e-8);
    let delta = std::array::from_fn::<_, 3, _>(|i| value[i] - center[i]);
    let longest = (0..3)
        .map(|i| (delta[i] / extent[i]).abs())
        .fold(1., f64::max);
    let q = std::array::from_fn::<_, 3, _>(|i| center[i] + delta[i] / longest);
    let clipped = [q[0] - q[2] + q[1], q[0] + q[2], q[0] - q[2] - q[1]];
    let motion = f64::from(((old_uv - uv) * Vec2::new(24., 16.)).length());
    let weight = 0.7 + 0.2 * (-motion).exp();
    std::array::from_fn(|i| now[i] * (1. - weight) + clipped[i] * weight)
}
