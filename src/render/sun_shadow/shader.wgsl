struct BgShadow { view_projection: mat4x4f, params: vec4f, center: vec4f };
@group(0) @binding(1) var<uniform> bg_shadow: BgShadow;
@group(0) @binding(2) var bg_shadow_depth: texture_depth_2d;
@group(0) @binding(3) var bg_shadow_sampler: sampler_comparison;
// Shadow only direct sunlight. No receiver bias is baked into geometry and
// emissive/voxel portal light remains unchanged when the map is disabled.
struct BgShadowReceiver { world: vec3f, projected: vec3f, uv: vec2f, slope: vec2f };
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
    return BgShadowReceiver(world, p.xyz, uv, slope);
}
fn bg_sun_visibility(receiver: BgShadowReceiver) -> f32 {
    if bg_shadow.params.z <= 0.0 { return 1.0; }
    let p = receiver.projected;
    let uv = receiver.uv;
    if p.z <= 0.0 || p.z >= 1.0 || any(uv <= vec2f(0.0)) || any(uv >= vec2f(1.0)) { return 1.0; }
    let distance_fade = 1.0 - smoothstep(bg_shadow.params.y * 0.75, bg_shadow.params.y, distance(receiver.world, bg_shadow.center.xyz));
    let edge = min(min(uv.x, uv.y), min(1.0-uv.x, 1.0-uv.y));
    let fade = distance_fade * smoothstep(0.0, 0.06, edge) * bg_shadow.params.z;
    if fade <= 0.0 { return 1.0; }
    // One bilinear comparison on Low; a deterministic 3x3 PCF on Medium/High.
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
    } else {
        for (var y = -1; y <= 1; y += 1) {
            for (var x = -1; x <= 1; x += 1) {
                let offset = vec2f(f32(x), f32(y)) * bg_shadow.params.x * bg_shadow.params.w;
                visibility += textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv + offset, p.z + dot(receiver.slope, offset) - bias);
            }
        }
        visibility /= 9.0;
    }
    return mix(1.0, visibility, fade);
}
