// Pure normal-frame and reflected-light math, shared by runtime and GPU regressions.
fn bg_normal_frame_supported(flat: vec3f, px: vec3f, py: vec3f) -> bool {
    let geometric = cross(px,py);
    let area = dot(geometric,geometric);
    let alignment = dot(flat,geometric);
    // Crossed plants intentionally shade upward on vertical cards. Projecting
    // their UV axes onto that shading plane collapses a basis vector to zero.
    return area > 0.000000000000000001
        && alignment*alignment > dot(flat,flat)*area*0.00000001;
}
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
    if abs(ux.x * uy.y - ux.y * uy.x) < 0.000000000001
        || !bg_normal_frame_supported(flat,px,py) { return flat; }
    return normalize(bg_texture_frame(flat, px, py, ux, uy) * tangent_normal);
}

// Height 1 is the original face plane; lower heights recede into the block.
// Fade undersampled and distant detail, and suppress unstable grazing offsets.
fn bg_parallax_ray(view: vec3f, distance: f32, mip: f32, settings: vec4f) -> vec2f {
    let scale = settings.x * (1.0-smoothstep(settings.y * 0.5, settings.y, distance))
        * (1.0-smoothstep(2.0, 4.0, mip)) * smoothstep(0.08, 0.20, view.z);
    return view.xy / max(view.z, 0.12) * scale;
}

fn bg_specular_light(normal: vec3f, v: vec3f, sun: vec4f, sky: f32,
    visibility: f32, albedo: vec3f, specular: vec4f, sun_radiance: vec3f) -> vec3f {
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
    return min(reflection, vec3f(8.0)) * nl * sun_radiance * sky * visibility;
}

// Crossed plants deliberately use upward diffuse normals. Only their thin
// transmission lobe uses the real card plane; cube leaves retain their mapped
// surface normal. Camera orientation removes raster-winding dependence.
fn bg_thin_transmission_normal(flat_normal: vec3f, normal: vec3f, geometric: vec3f, view: vec3f) -> vec3f {
    let size = length(geometric);
    if size <= 0.00000001 { return normal; }
    let plane = geometric / size;
    if abs(dot(flat_normal, plane)) >= 0.25 { return normal; }
    return select(-plane, plane, dot(plane, view) >= 0.0);
}

fn bg_thin_direct(normal: vec3f, transmission_normal: vec3f, sun: vec4f,
    sky: f32, wrap: f32, transmission: f32, sun_radiance: vec3f) -> vec3f {
    let direction = normalize(sun.xyz);
    let cosine = dot(normal, direction);
    let diffuse = max((cosine + wrap) / (1.0 + wrap), 0.0);
    let through = max(-dot(transmission_normal, direction), 0.0) * transmission;
    return sky * min(diffuse + through, 1.0) * sun_radiance;
}

// oldPBR: R smoothness, G metalness. Missing companions retain legacy diffuse.
// Metals redirect their base color to reflection rather than doubling it as diffuse.
fn bg_material_diffuse_weight(specular: vec4f, nv: f32) -> f32 {
    let roughness = max(0.15,1.0-specular.r);
    let fresnel = 0.04 + (max(1.0-roughness,0.04)-0.04)*pow(1.0-clamp(nv,0.0,1.0),5.0);
    return select(1.0, (1.0-fresnel) * (1.0-clamp(specular.g,0.0,1.0)), specular.a > 0.0);
}

// Fresnel-weighted broad environment approximation, not a reflection probe.
// Roughness reduces the grazing boost and integrated lobe; radiance is already
// blurred by the caller. Local light is separate from outdoor visibility, so a
// lit room can reflect its glow without importing an outdoor sky through walls.
fn bg_environment_specular(normal: vec3f, view: vec3f, albedo: vec3f,
    specular: vec4f, sky_radiance: vec3f, sky_visibility: f32,
    local_radiance: vec3f, local_visibility: f32) -> vec3f {
    if specular.a == 0.0 { return vec3f(0.0); }
    let roughness = max(0.15, 1.0-specular.r);
    let nv = clamp(dot(normal,view),0.0,1.0);
    let f0 = mix(vec3f(0.04),albedo,clamp(specular.g,0.0,1.0));
    let fresnel = f0 + (max(vec3f(1.0-roughness),f0)-f0)*pow(1.0-nv,5.0);
    let energy = 1.0-0.5*roughness*roughness;
    return fresnel * energy * (max(sky_radiance,vec3f(0.0))*clamp(sky_visibility,0.0,1.0)
        + max(local_radiance,vec3f(0.0))*clamp(local_visibility,0.0,1.0));
}

// Transport available without any sky, sun direction, normal map or custom hook.
// Compute before vertex interpolation so a curved normal cannot turn sunlight
// into an apparent indoor light source through non-linear diffuse subtraction.
fn bg_local_material_radiance(local_radiance: vec3f, bounce: vec3f, glow_bounce: vec3f,
    daylight_mix: f32) -> vec3f {
    return local_radiance + mix(glow_bounce,bounce,daylight_mix)*1.35;
}
