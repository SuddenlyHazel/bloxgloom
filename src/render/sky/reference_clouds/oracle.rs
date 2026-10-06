//! Double-precision defaults evaluated independently of runtime WGSL.
pub(super) fn density(base: f64, detail: f64, gradient: f64, reveal: f64, rain: f64) -> f64 {
    let edge = (gradient - 0.125).abs() * if gradient > 0.125 { 1.14 } else { 8. };
    let coverage = 4. * edge.powi(2);
    let signal = 21. * (0.9524 * base + 0.0476 * detail);
    let wet = (1. - rain * 0.33) * (signal - coverage) + rain * 0.33 * (21. - 2.5 * coverage);
    let value = (wet - 10. - 3. * reveal).max(0.) * 0.5 * (1. - rain * 0.75);
    value / (value * value + 0.5).sqrt()
}
pub(super) fn dither(pixel: [f64; 2], frame: f64) -> f64 {
    let b = |scale: f64| {
        ((pixel[0] * scale).floor() * 0.5 + (pixel[1] * scale).floor() * 0.75).rem_euclid(1.)
    };
    (b(1.) + b(0.5) / 4. + b(0.25) / 16. + frame * 0.618).rem_euclid(1.)
}
pub(super) fn integrate(
    origin: [f64; 3],
    ray: [f64; 3],
    pixel: [f64; 2],
    rain: f64,
    night: bool,
) -> [f64; 4] {
    integrate_case(origin, ray, pixel, rain, night, origin[1], false)
}

pub(super) fn integrate_case(
    origin: [f64; 3],
    ray: [f64; 3],
    pixel: [f64; 2],
    rain: f64,
    night: bool,
    eye_height: f64,
    fade_faster: bool,
) -> [f64; 4] {
    let lower = (192. - origin[1]) / ray[1];
    let upper = (252. - origin[1]) / ray[1];
    let nearest = lower.min(upper).max(0.);
    let furthest = lower.max(upper);
    if furthest < 0. {
        return [0., 0., 0., 1.];
    }
    let scaling = (((eye_height - 222.).abs() / 30. - 1.) * 0.625).clamp(0., 1.);
    let step = 30. / (1. + 4. * ray[1] * ray[1] * scaling);
    let count = ((furthest - nearest) / step).min(32.) as usize + 1;
    let d = dither(pixel, 17.);
    let vl = ray[1];
    let reveal = (2. * vl - 1.).clamp(0., 1.).powi(12) * (1. - rain);
    let half = (vl + 1.) / 2.;
    let scattering = half.powi(6);
    let mut opacity = 0.;
    let mut lighting = 0.;
    let mut fade = 1.;
    for i in 0..count {
        if opacity > 0.99 {
            break;
        }
        let distance = nearest + step * (i as f64 + d);
        let y = origin[1] + ray[1] * distance;
        let gradient = ((y - 192.) / 60.).clamp(0., 1.);
        let mut noise = if (192. ..=252.).contains(&y) {
            density(0.6, 0.4, gradient, reveal, rain)
        } else {
            0.
        };
        let xz = (ray[0] * ray[0] + ray[2] * ray[2]).sqrt() * distance * (10. / 73.);
        let sample_light = (0.8 * gradient.powf(1.125 * half * half + 0.875) + 0.2)
            * (1. - noise.powf(4. - 3. * vl));
        let fade_end = if fade_faster { 80. } else { 240. };
        let sample_fade = ((fade_end - xz) / (fade_end - 32.)).clamp(0., 1.);
        fade *= 1. - (1. - sample_fade) * noise * (1. - opacity);
        if xz > fade_end {
            noise = 0.;
        }
        lighting = lighting + (sample_light - lighting) * noise * (1. - opacity * opacity);
        opacity += noise * (1. - opacity);
    }
    lighting += (1. - lighting) * (1. - opacity * opacity) * scattering * 0.5;
    lighting *= 1. - 0.9 * rain;
    let mut source_light = if night {
        [96. / 255. * 0.3, 192. / 255. * 0.3, 0.3]
    } else {
        [196. / 255. * 1.4, 220. / 255. * 1.4, 1.4]
    };
    let mut ambient = if night {
        [96. / 255. * 0.18, 192. / 255. * 0.18, 0.18]
    } else {
        [120. / 255. * 0.6, 172. / 255. * 0.6, 0.6]
    };
    for color in [&mut source_light, &mut ambient] {
        let gray = color[0] * 0.299 + color[1] * 0.587 + color[2] * 0.114;
        for (v, w) in color
            .iter_mut()
            .zip([176. / 255. * 1.2, 224. / 255. * 1.2, 1.2])
        {
            *v = *v * (1. - rain) + gray * w * rain;
        }
    }
    let alpha = (opacity * fade).powi(2);
    let mut result = [0.; 4];
    for c in 0..3 {
        let low = ambient[c] * ambient[c] * if night { 0.5 } else { 0.8 };
        let high = source_light[c] * source_light[c] * (0.85 + 1.15 * scattering);
        result[c] = (low + (high - low) * lighting)
            * (1. - 0.4 * rain)
            * (0.5 - if night { 0.25 * (1. - rain) } else { 0. })
            * alpha
            * ((eye_height + 70.) / 8.).clamp(0., 1.);
    }
    result[3] = 1. - alpha;
    result
}

pub(super) fn sample(pixels: &[u8], position: [f64; 3], wind: [f64; 2]) -> f64 {
    let sample = |uv: [f64; 2], channel: usize| {
        let texel = uv.map(|v| v * 2. - 0.5);
        let cell = texel.map(f64::floor);
        let fraction = [texel[0] - cell[0], texel[1] - cell[1]];
        let fetch = |dx: i32, dy: i32| {
            let x = (cell[0] as i32 + dx).rem_euclid(2) as usize;
            let y = (cell[1] as i32 + dy).rem_euclid(2) as usize;
            f64::from(pixels[(y * 2 + x) * 4 + channel]) / 255.
        };
        let bottom = fetch(0, 0) * (1. - fraction[0]) + fetch(1, 0) * fraction[0];
        let top = fetch(0, 1) * (1. - fraction[0]) + fetch(1, 1) * fraction[0];
        bottom * (1. - fraction[1]) + top * fraction[1]
    };
    let gradient = ((position[1] - 192.) / 60.).clamp(0., 1.);
    let coord = [position[0] * (0.004 / 12.), position[2] * (0.004 / 12.)];
    let base = sample([coord[0] * 0.25 + wind[0], coord[1] * 0.25 + wind[1]], 0);
    let slices = gradient * 5.;
    let detailcoord = [
        coord[0] * 0.5 - wind[0] * 2. + slices.floor() * 0.04,
        coord[1] * 0.5 - wind[1] * 2. + slices.floor() * 0.04,
    ];
    let low = sample(detailcoord, 2);
    let high = sample(detailcoord.map(|v| v + 0.04), 2);
    density(
        base,
        low * (1. - slices.fract()) + high * slices.fract(),
        gradient,
        0.,
        0.,
    )
}
