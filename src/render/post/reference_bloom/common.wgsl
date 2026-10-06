// BSL composite4/5 default radius 3, strength 1, contrast 0.
fn bg_bloom_offset(level: u32, view: vec2f) -> vec2f {
    switch level {
        case 1u: { return vec2f(0.0); }
        case 2u: { return vec2f(0.5, 0.0) + vec2f(4.0, 0.0) * view; }
        case 3u: { return vec2f(0.5, 0.25) + vec2f(4.0) * view; }
        case 4u: { return vec2f(0.625, 0.25) + vec2f(8.0, 4.0) * view; }
        case 5u: { return vec2f(0.6875, 0.25) + vec2f(12.0, 4.0) * view; }
        case 6u: { return vec2f(0.625, 0.3125) + vec2f(8.0) * view; }
        default: { return vec2f(0.640625, 0.3125) + vec2f(12.0, 8.0) * view; }
    }
}
fn bg_bloom_radius_weight(level: u32) -> f32 {
    let weights = array<f32, 7>(4.0, 3.18, 2.52, 2.0, 1.59, 1.26, 1.0);
    return weights[level - 1u];
}
fn bg_bloom_decode(encoded: vec3f) -> vec3f {
    let square = encoded * encoded;
    return square * square * 32.0;
}
