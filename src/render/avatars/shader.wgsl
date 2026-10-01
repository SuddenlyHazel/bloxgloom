struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) local: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) part: u32,
    @location(3) origin: vec3<f32>,
    @location(4) cosmetics: vec4<u32>,
    @location(5) light_levels: vec4<u32>,
    @location(6) bounce: vec4<u32>,
    @location(7) pose: vec4<f32>,
    @location(8) color: vec3<f32>,
    @location(9) tint: vec3<f32>,
    @location(10) glow_bounce: vec4<u32>,
    @location(11) orientation: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) distance: f32,
    @location(2) sky: f32,
};

// REGISTERED_PALETTES

@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    var local = input.local;
    if input.part == 10u { local.y += max(0.0, input.pose.y) * 0.045; }
    if input.part == 11u { local.y += max(0.0, -input.pose.y) * 0.045; }
    if input.part >= 5u && input.part != 12u {
        local.y *= 1.0 - input.pose.w;
        local.x *= 1.0 + input.pose.w * 0.5;
        local.z *= 1.0 + input.pose.w * 0.5;
        if input.part != 10u && input.part != 11u { local.y += input.pose.z; }
        // Upper-ear sway follows the same continuous pose as the body.
        if input.local.y > 0.65 { local.z += input.pose.z * 2.0; }
    }
    let c = cos(input.pose.x);
    let s = sin(input.pose.x);
    var rotated = vec3<f32>(local.x * c + local.z * s, local.y, local.z * c - local.x * s);
    var normal = vec3<f32>(input.normal.x * c + input.normal.z * s, input.normal.y, input.normal.z * c - input.normal.x * s);
    if input.part == 12u {
        let q = normalize(input.orientation);
        rotated = local + 2.0 * cross(q.xyz, cross(q.xyz, local) + q.w * local);
        normal = input.normal + 2.0 * cross(q.xyz, cross(q.xyz, input.normal) + q.w * input.normal);
    }
    let world = rotated + input.origin;
    normal = normalize(normal);
    output.clip = camera.view_projection * vec4<f32>(world, 1.0);
    var albedo = SKINS[min(input.cosmetics.x, 31u)];
    if input.part == 1u { albedo = SHIRTS[min(input.cosmetics.y, 31u)]; }
    if input.part == 2u { albedo = PANTS[min(input.cosmetics.z, 31u)]; }
    if input.part == 3u { albedo = mix(PANTS[min(input.cosmetics.z, 31u)], vec3<f32>(0.10, 0.08, 0.07), 0.65); }
    if input.part == 4u { albedo = vec3<f32>(0.025, 0.035, 0.045); }
    if input.part >= 5u { albedo = input.color; }
    let sky = f32(input.light_levels.x) / 15.0;
    let glow = f32(input.light_levels.y) / 15.0;
    let bounce = vec3<f32>(f32(input.bounce.x), f32(input.bounce.y), f32(input.bounce.z)) / 255.0;
    let glow_bounce = vec3<f32>(f32(input.glow_bounce.x), f32(input.glow_bounce.y), f32(input.glow_bounce.z)) / 255.0;
    let sun = max(dot(normal, normalize(camera.sun.xyz)), 0.0);
    let light = vec3<f32>(0.012, 0.015, 0.022)
        + sky * camera.sun.w * (vec3<f32>(0.31, 0.40, 0.53) + sun * vec3<f32>(0.77, 0.66, 0.47))
        + glow * glow * vec3<f32>(1.0, 0.57, 0.23)
        + mix(glow_bounce, bounce, camera.sun.w) * 1.35;
    output.color = albedo * input.tint * light;
    output.distance = output.clip.w;
    output.sky = sky;
    return output;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let fog = smoothstep(38.0, 135.0, input.distance);
    let fog_sky = mix(vec3<f32>(0.006, 0.009, 0.016), camera.horizon.xyz, input.sky);
    return vec4<f32>(mix(input.color, fog_sky, fog), 1.0);
}
