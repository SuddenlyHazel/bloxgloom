struct BgShadow { view_projection: mat4x4f, params: vec4f, center: vec4f };
@group(0) @binding(1) var<uniform> bg_shadow: BgShadow;
@group(0) @binding(2) var bg_shadow_depth: texture_depth_2d;
@group(0) @binding(3) var bg_shadow_sampler: sampler_comparison;
// Shadow only direct sunlight. No receiver bias is baked into geometry and
// emissive/voxel portal light remains unchanged when the map is disabled.
fn bg_sun_visibility(world: vec3f) -> f32 {
    if bg_shadow.params.z <= 0.0 { return 1.0; }
    let p = bg_shadow.view_projection * vec4f(world, 1.0);
    let uv = p.xy * vec2f(0.5, -0.5) + vec2f(0.5);
    if p.z <= 0.0 || p.z >= 1.0 || any(uv <= vec2f(0.0)) || any(uv >= vec2f(1.0)) { return 1.0; }
    let distance_fade = 1.0 - smoothstep(bg_shadow.params.y * 0.75, bg_shadow.params.y, distance(world, bg_shadow.center.xyz));
    let edge = min(min(uv.x, uv.y), min(1.0-uv.x, 1.0-uv.y));
    let fade = distance_fade * smoothstep(0.0, 0.06, edge) * bg_shadow.params.z;
    if fade <= 0.0 { return 1.0; }
    // One bilinear comparison on Low; a deterministic 3x3 PCF on Medium/High.
    // Fixed light-space taps avoid frame noise and swimming rotation patterns.
    var visibility = 0.0;
    if bg_shadow.params.w <= 0.0 {
        visibility = textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv, p.z - 0.000025);
    } else {
        for (var y = -1; y <= 1; y += 1) {
            for (var x = -1; x <= 1; x += 1) {
                visibility += textureSampleCompareLevel(bg_shadow_depth, bg_shadow_sampler, uv + vec2f(f32(x), f32(y)) * bg_shadow.params.x * bg_shadow.params.w, p.z - 0.000025);
            }
        }
        visibility /= 9.0;
    }
    return mix(1.0, visibility, fade);
}
