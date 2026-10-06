struct BgSceneContact { bounds: vec4f, center: vec4f, light: vec4f };
struct BgShadow { view_projection: mat4x4f, params: vec4f, center: vec4f, contact_params: vec4f, contacts: array<BgSceneContact, 32>, soft_params: vec4f, reference_params:vec4f };
@group(0) @binding(1) var<uniform> bg_shadow: BgShadow;
@group(0) @binding(2) var bg_shadow_depth: texture_depth_2d;
@group(0) @binding(3) var bg_shadow_sampler: sampler_comparison;
// Shadow only direct sunlight. No receiver bias is baked into geometry and
// emissive/voxel portal light remains unchanged when the map is disabled.
struct BgShadowReceiver { world: vec3f, projected: vec3f, uv: vec2f, slope: vec2f, geometric:vec3f };
// Call before any alpha discard. Derivatives describe the actual geometric
// receiver, independent of normal maps and camera-dependent parallax UVs.
fn bg_shadow_receiver(world: vec3f) -> BgShadowReceiver {
    let p = bg_shadow.view_projection * vec4f(world, 1.0);
    let uv = p.xy * vec2f(0.5, -0.5) + vec2f(0.5);
    let dx = dpdx(p.xyz);
    let dy = dpdy(p.xyz);
    let ux = dx.xy * vec2f(0.5, -0.5);
    let uy = dy.xy * vec2f(0.5, -0.5);
    let determinant = ux.x * uy.y - ux.y * uy.x;
    var slope = vec2f(0.0);
    if abs(determinant) > 0.00000000000000000001 {
        slope = vec2f(dx.z * uy.y - dy.z * ux.y, dy.z * ux.x - dx.z * uy.x) / determinant;
    }
    var geometric=vec3f(0.0,1.0,0.0);
    if bg_shadow.reference_params.x>0.5 {geometric=normalize(cross(dpdx(world),dpdy(world)));}
    return BgShadowReceiver(world, p.xyz, uv, slope,geometric);
}
// Estimate a blocker depth using comparison samples only. Naga's GL backend
// cannot textureLoad depth or sample it with a non-comparison sampler. A fixed
// six-step search is portable, converges within 0.375 world units, and needs no
// extra shadow texture, binding, or backend-specific fallback shader.
fn bg_blocker_gap(uv: vec2f, receiver_depth: f32, occlusion: f32) -> f32 {
    var near_gap = 0.0;
    var far_gap = bg_shadow.soft_params.z;
    for (var step = 0; step < 6; step++) {
        let gap = (near_gap + far_gap) * 0.5;
        let visibility = textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv,
            receiver_depth - gap / bg_shadow.center.w);
        // Resolve the median depth among the covered part of a bilinear tap.
        // Partial coverage must not be mistaken for a blocker at the receiver.
        if visibility < 1.0 - occlusion * 0.5 { near_gap = gap; }
        else { far_gap = gap; }
    }
    return (near_gap + far_gap) * 0.5;
}

fn bg_penumbra_radius(receiver: BgShadowReceiver, bias: f32) -> f32 {
    let search_taps = array<vec2f, 9>(vec2f(0.0), vec2f(-0.85, 0.0),
        vec2f(0.85, 0.0), vec2f(0.0, -0.85), vec2f(0.0, 0.85),
        vec2f(-0.4, -0.4), vec2f(0.4, -0.4),
        vec2f(-0.4, 0.4), vec2f(0.4, 0.4));
    let search_radius = bg_shadow.soft_params.y * bg_shadow.params.x;
    var gap_sum = 0.0;
    var coverage = 0.0;
    for (var tap = 0u; tap < 9u; tap++) {
        let offset = search_taps[tap] * search_radius;
        let uv = receiver.uv + offset;
        // Correct the search plane as well as final PCF. Coplanar sloped
        // receivers are not blockers and cannot make their own penumbra.
        let receiver_depth = receiver.projected.z + dot(receiver.slope, offset) - bias;
        let occlusion = 1.0 - textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv, receiver_depth);
        if occlusion > 0.01 {
            gap_sum += occlusion * bg_blocker_gap(uv, receiver_depth, occlusion);
            coverage += occlusion;
        }
    }
    // Orthographic depth range is three times the map's world-space width.
    // A directional area light's penumbra grows with blocker/receiver separation,
    // never camera distance or the block's normal-map detail.
    let texel_world = bg_shadow.center.w * bg_shadow.params.x / 3.0;
    let gap = gap_sum / max(coverage, 0.0001);
    let penumbra = gap * bg_shadow.soft_params.x / max(texel_world, 0.00001);
    let base = bg_shadow.params.w;
    return min(sqrt(base * base + penumbra * penumbra), bg_shadow.soft_params.y);
}

fn bg_soft_sun_filter(receiver: BgShadowReceiver, bias: f32, radius: f32) -> f32 {
    // Fixed symmetric light-space disk. No screen/frame hashes, jitter, rotation,
    // or random noise: static world points keep the same sampling pattern.
    let taps = array<vec2f, 16>(
        vec2f(0.1768, 0.1768), vec2f(-0.1768, 0.1768),
        vec2f(-0.1768, -0.1768), vec2f(0.1768, -0.1768),
        vec2f(0.6250, 0.0), vec2f(0.0, 0.6250),
        vec2f(-0.6250, 0.0), vec2f(0.0, -0.6250),
        vec2f(0.9239, 0.3827), vec2f(0.3827, 0.9239),
        vec2f(-0.3827, 0.9239), vec2f(-0.9239, 0.3827),
        vec2f(-0.9239, -0.3827), vec2f(-0.3827, -0.9239),
        vec2f(0.3827, -0.9239), vec2f(0.9239, -0.3827));
    var visibility = 0.0;
    for (var tap = 0u; tap < 16u; tap++) {
        let offset = taps[tap] * bg_shadow.params.x * radius;
        visibility += textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler,
            receiver.uv + offset, receiver.projected.z + dot(receiver.slope, offset) - bias);
    }
    return visibility / 16.0;
}

fn bg_enhanced_sun_visibility(receiver: BgShadowReceiver) -> f32 {
    if bg_shadow.params.z <= 0.0 { return 1.0; }
    let p = receiver.projected;
    let uv = receiver.uv;
    if p.z <= 0.0 || p.z >= 1.0 || any(uv <= vec2f(0.0)) || any(uv >= vec2f(1.0)) { return 1.0; }
    let distance_fade = 1.0 - smoothstep(bg_shadow.params.y * 0.75, bg_shadow.params.y, distance(receiver.world, bg_shadow.center.xyz));
    let edge = min(min(uv.x, uv.y), min(1.0-uv.x, 1.0-uv.y));
    let fade = distance_fade * smoothstep(0.0, 0.06, edge) * bg_shadow.params.z;
    if fade <= 0.0 { return 1.0; }
    // Low retains one comparison. Zero softness retains the original tent PCF.
    // Fixed light-space taps avoid frame noise and swimming rotation patterns.
    // Each tap compares against the receiver plane at that tap, not the center
    // depth. Caster slope bias already covers the larger texel-depth axis;
    // only the smaller axis remains for a bilinear comparison's diagonal.
    // Avoid paying that allowance twice and pushing contact shadows away.
    let slope = abs(receiver.slope);
    let bias = 0.000025 + min(slope.x, slope.y) * bg_shadow.params.x;
    var visibility = 0.0;
    if bg_shadow.params.w <= 0.0 {
        visibility = textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv, p.z - bias);
    } else if bg_shadow.soft_params.x <= 0.0 {
        for (var y = -1; y <= 1; y += 1) {
            for (var x = -1; x <= 1; x += 1) {
                let offset = vec2f(f32(x), f32(y)) * bg_shadow.params.x * bg_shadow.params.w;
                // Separable tent weights preserve tight contact while gently
                // smoothing the outer edge. Same nine taps, no temporal noise.
                let weight = f32(2 - abs(x)) * f32(2 - abs(y));
                visibility += weight * textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv + offset, p.z + dot(receiver.slope, offset) - bias);
            }
        }
        visibility /= 16.0;
    } else {
        let radius = bg_penumbra_radius(receiver, bias);
        visibility = bg_soft_sun_filter(receiver, bias, radius);
    }
    return mix(1.0, visibility, fade);
}
