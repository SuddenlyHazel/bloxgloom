// Pure normal-frame and reflected-light math, shared by runtime and GPU regressions.
fn bg_texture_frame(flat: vec3f, px: vec3f, py: vec3f, ux: vec2f, uy: vec2f) -> mat3x3f {
    let determinant = ux.x * uy.y - ux.y * uy.x;
    let n = normalize(flat);
    let raw_t = (px * uy.y - py * ux.y) / determinant;
    let raw_b = (py * ux.x - px * uy.x) / determinant;
    let t = normalize(raw_t - n * dot(n, raw_t));
    let b = normalize(raw_b - n * dot(n, raw_b));
    return mat3x3f(t, b, n);
}
fn bg_normal_frame(flat: vec3f, px: vec3f, py: vec3f, ux: vec2f, uy: vec2f,
    tangent_normal: vec3f) -> vec3f {
    if abs(ux.x * uy.y - ux.y * uy.x) < 0.000000000001 { return flat; }
    return normalize(bg_texture_frame(flat, px, py, ux, uy) * tangent_normal);
}

// Height 1 is the original face plane; lower heights recede into the block.
// Fade undersampled and distant detail, and suppress unstable grazing offsets.
fn bg_parallax_ray(view: vec3f, distance: f32, mip: f32) -> vec2f {
    let scale = 0.035 * (1.0-smoothstep(16.0, 32.0, distance))
        * (1.0-smoothstep(2.0, 4.0, mip)) * smoothstep(0.08, 0.20, view.z);
    return view.xy / max(view.z, 0.12) * scale;
}

fn bg_specular_light(normal: vec3f, v: vec3f, sun: vec4f, sky: f32,
    visibility: f32, albedo: vec3f, specular: vec4f) -> vec3f {
    if specular.a == 0.0 || sky == 0.0 || visibility == 0.0 { return vec3f(0.0); }
    let n = normalize(normal);
    let l = normalize(sun.xyz);
    let half_vector = l + v;
    let h = half_vector / max(length(half_vector), 0.0001);
    let nl = max(dot(n, l), 0.0);
    let nv = max(dot(n, v), 0.0);
    let nh = max(dot(n, h), 0.0);
    let vh = max(dot(v, h), 0.0);
    let roughness = max(0.15, 1.0 - specular.r);
    let alpha = roughness * roughness;
    let a2 = alpha * alpha;
    let denominator = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / max(3.14159265 * denominator * denominator, 0.00001);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let geometry = nl / max(nl * (1.0-k) + k, 0.0001)
                 * nv / max(nv * (1.0-k) + k, 0.0001);
    let f0 = mix(vec3f(0.04), albedo, specular.g);
    let fresnel = f0 + (vec3f(1.0)-f0) * pow(1.0-vh, 5.0);
    let reflection = distribution * geometry * fresnel / max(4.0 * nl * nv, 0.0001);
    return min(reflection, vec3f(8.0)) * nl * vec3f(0.72, 0.67, 0.56)
        * sun.w * sky * visibility;
}
