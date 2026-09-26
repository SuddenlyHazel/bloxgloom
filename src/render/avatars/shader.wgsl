struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) local: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) part: u32,
    @location(3) origin: vec3<f32>,
    @location(4) cosmetics: vec4<u32>,
    @location(5) light_levels: vec4<u32>,
    @location(6) bounce: vec4<u32>,
    @location(7) pose: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) distance: f32,
    @location(2) sky: f32,
};

const SKINS = array<vec3<f32>, 6>(
    vec3<f32>(0.91, 0.68, 0.49), vec3<f32>(0.75, 0.50, 0.32),
    vec3<f32>(0.59, 0.37, 0.24), vec3<f32>(0.40, 0.25, 0.18),
    vec3<f32>(0.97, 0.79, 0.61), vec3<f32>(0.67, 0.43, 0.32),
);
const SHIRTS = array<vec3<f32>, 8>(
    vec3<f32>(0.13, 0.39, 0.69), vec3<f32>(0.70, 0.23, 0.18),
    vec3<f32>(0.19, 0.55, 0.38), vec3<f32>(0.67, 0.49, 0.17),
    vec3<f32>(0.42, 0.30, 0.61), vec3<f32>(0.17, 0.57, 0.62),
    vec3<f32>(0.63, 0.34, 0.45), vec3<f32>(0.38, 0.46, 0.55),
);
const PANTS = array<vec3<f32>, 6>(
    vec3<f32>(0.14, 0.20, 0.35), vec3<f32>(0.22, 0.25, 0.29),
    vec3<f32>(0.30, 0.25, 0.21), vec3<f32>(0.23, 0.32, 0.26),
    vec3<f32>(0.34, 0.29, 0.43), vec3<f32>(0.25, 0.33, 0.43),
);

@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    var local = input.local;
    if input.part == 10u { local.y += max(0.0, input.pose.y) * 0.045; }
    if input.part == 11u { local.y += max(0.0, -input.pose.y) * 0.045; }
    let c = cos(input.pose.x);
    let s = sin(input.pose.x);
    let world = vec3<f32>(local.x * c + local.z * s, local.y, local.z * c - local.x * s) + input.origin;
    let normal = vec3<f32>(input.normal.x * c + input.normal.z * s, input.normal.y, input.normal.z * c - input.normal.x * s);
    output.clip = camera.view_projection * vec4<f32>(world, 1.0);
    var albedo = SKINS[input.cosmetics.x % 6u];
    if input.part == 1u { albedo = SHIRTS[input.cosmetics.y % 8u]; }
    if input.part == 2u { albedo = PANTS[input.cosmetics.z % 6u]; }
    if input.part == 3u { albedo = mix(PANTS[input.cosmetics.z % 6u], vec3<f32>(0.10, 0.08, 0.07), 0.65); }
    if input.part == 4u { albedo = vec3<f32>(0.025, 0.035, 0.045); }
    if input.part == 5u { albedo = vec3<f32>(0.49, 0.77, 0.58); }
    if input.part == 6u || input.part >= 10u { albedo = vec3<f32>(0.96, 0.88, 0.68); }
    if input.part == 7u { albedo = vec3<f32>(0.93, 0.49, 0.51); }
    if input.part == 8u { albedo = vec3<f32>(0.025, 0.045, 0.05); }
    if input.part == 9u { albedo = vec3<f32>(1.0, 0.97, 0.86); }
    let sky = f32(input.light_levels.x) / 15.0;
    let glow = f32(input.light_levels.y) / 15.0;
    let bounce = vec3<f32>(f32(input.bounce.x), f32(input.bounce.y), f32(input.bounce.z)) / 255.0;
    let sun = max(dot(normal, normalize(WORLD_SUN_DIRECTION)), 0.0);
    let light = vec3<f32>(0.012, 0.015, 0.022)
        + sky * (vec3<f32>(0.31, 0.40, 0.53) + sun * vec3<f32>(0.77, 0.66, 0.47))
        + glow * glow * vec3<f32>(1.0, 0.57, 0.23)
        + bounce * 1.35;
    output.color = albedo * light;
    output.distance = output.clip.w;
    output.sky = sky;
    return output;
}

@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let fog = smoothstep(38.0, 135.0, input.distance);
    let fog_sky = mix(vec3<f32>(0.006, 0.009, 0.016), vec3<f32>(0.59, 0.72, 0.82), input.sky);
    return vec4<f32>(mix(input.color, fog_sky, fog), 1.0);
}
