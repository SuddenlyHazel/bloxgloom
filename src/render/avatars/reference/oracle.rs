//! Independent f64 expression of default entity GetLighting; no imported WGSL.
use glam::DVec3;
pub(super) fn map(v: DVec3, f: impl Fn(f64) -> f64) -> DVec3 {
    DVec3::new(f(v.x), f(v.y), f(v.z))
}
pub(super) fn encoded(value: f64) -> f64 {
    if value <= 0.0031308 {
        value.max(0.0) * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}
pub(super) fn entity(
    encoded_rgb: DVec3,
    tint: DVec3,
    normal: DVec3,
    conditions: [f64; 5],
) -> DVec3 {
    let [block, sky, shadow, rain, visible] = conditions;
    let albedo = map(encoded_rgb * map(tint, encoded), |v| v.powf(2.2));
    let light = DVec3::new(196., 220., 255.) * (1.4 / 255.);
    let light = light * light;
    let ambient = DVec3::new(120., 172., 255.) * (0.6 / 255.);
    let ambient = ambient * ambient;
    let full_shadow = shadow * (normal.y * 1.01 - 0.01).clamp(0.0, 1.0);
    let scene = (ambient * sky).lerp(light, full_shadow * (1.0 - 0.95 * rain)) * sky * sky;
    let block = block.min(0.9333);
    let raw = DVec3::new(255., 212., 160.) * (0.85 / 255.);
    let curve = block.powi(10) * 1.6 + block * 0.6;
    let block_light = raw * raw * curve * curve;
    let minimum = (128.0f64 * 0.5 / 255.0).powi(2) * 0.04 * (1.0 - sky * sky);
    let peak = albedo.max_element();
    let balanced = albedo / (1.0 + peak * 0.25);
    let diffuse = ((0.25 * normal.y + 0.75)
        + (0.667 - normal.x.abs()) * (1.0 - normal.y.abs()) * 0.15)
        .powi(2);
    let color = balanced * (scene + block_light + DVec3::splat(minimum)) * diffuse;
    let mut amount = 1.0
        - (full_shadow * 3.0f64.sqrt() / 3.0)
            .sqrt()
            .mul_add(sky, 0.0)
            .max(sky)
            .sqrt()
            * visible
            * (1.0 - rain * 0.7);
    let value = ((1.0 - block).powi(2) - 0.25) / 0.75;
    let v = value.clamp(0.0, 1.0);
    amount *= v * v * (3.0 - 2.0 * v);
    amount = 1.0 - amount;
    let weather = DVec3::new(176., 224., 255.) * (1.2 / 255.);
    let night = DVec3::new(96., 192., 255.) * (0.3 / 255.);
    let hue = (weather * weather + DVec3::splat(1e-6)).normalize().lerp(
        (night * night + DVec3::splat(1e-6)).normalize(),
        (1.0 - visible) * (1.0 - rain),
    );
    let tint = DVec3::splat(0.4).lerp(hue, sky.sqrt()) * 1.7;
    let gray = color.dot(DVec3::new(0.299, 0.587, 0.114));
    color.lerp(gray * tint, 0.4).lerp(color, amount)
}
