struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f, cloud: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) local: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) part: u32,
    @location(3) origin: vec3<f32>,
    @location(4) cosmetics: vec4<u32>,
    @location(5) light_levels: vec2<u32>,
    @location(6) bounce: vec4<u32>,
    @location(7) pose: vec4<f32>,
    @location(8) color: vec3<f32>,
    @location(9) tint: vec3<f32>,
    @location(10) glow_bounce: vec4<u32>,
    @location(11) orientation: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) direct: vec3f,
    @location(2) sky: f32,
    @location(3) world_position: vec3f,
    @location(4) indirect: vec3f,
    @location(5) normal: vec3f,
    @location(6) local_radiance: vec3f,
    @location(7) local_direction: vec3f,
    @location(8) surface_color: vec3f,
};

// REGISTERED_PALETTES

fn avatar_vertex(input: VertexInput, shadow: bool) -> VertexOutput {
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
    if shadow { output.clip = bg_shadow.view_projection * vec4f(world, 1.0); }
    var albedo = SKINS[min(input.cosmetics.x, 31u)];
    if input.part == 1u { albedo = SHIRTS[min(input.cosmetics.y, 31u)]; }
    if input.part == 2u { albedo = PANTS[min(input.cosmetics.z, 31u)]; }
    if input.part == 3u { albedo = mix(PANTS[min(input.cosmetics.z, 31u)], vec3<f32>(0.10, 0.08, 0.07), 0.65); }
    if input.part == 4u { albedo = vec3<f32>(0.025, 0.035, 0.045); }
    if input.part >= 5u { albedo = input.color; }
    let sky = f32(input.light_levels.x & 255u) / 15.0;
    let glow = f32((input.light_levels.x >> 8u) & 255u) / 15.0;
    let packed_color = vec3f(unpack4x8unorm(input.light_levels.x).zw, unpack4x8unorm(input.light_levels.y).x);
    output.normal = normal;
    output.local_radiance = packed_color * glow * glow;
    output.surface_color = albedo * input.tint;
    output.local_direction = unpack4x8snorm(input.light_levels.y).yzw;
    let bounce = vec3<f32>(f32(input.bounce.x), f32(input.bounce.y), f32(input.bounce.z)) / 255.0;
    let glow_bounce = vec3<f32>(f32(input.glow_bounce.x), f32(input.glow_bounce.y), f32(input.glow_bounce.z)) / 255.0;
    let light = bg_surface_light(normal, camera.sun, sky, vec3f(0.0), bounce, glow_bounce, 1.0);
    output.color = albedo * input.tint * light;
    output.indirect = albedo * input.tint * bg_indirect_light(normal,camera.sun,sky,bounce,glow_bounce);
    output.direct = albedo * input.tint * bg_direct_light(normal, camera.sun, sky);
    output.world_position = world;
    output.sky = sky;
    return output;
}

@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    return avatar_vertex(input, false);
}

@vertex fn vs_shadow(input: VertexInput) -> @builtin(position) vec4f {
    return avatar_vertex(input, true).clip;
}

@fragment fn fs_main(input: VertexOutput) -> BgSceneOutput {
    let receiver = bg_shadow_receiver(input.world_position);
    // Local visibility is evaluated per fragment, retaining voxel scattering.
    let local_light = bg_shadowed_local_light(input.world_position, normalize(input.normal), input.local_radiance, input.local_direction);
    let visibility = bg_sun_visibility(receiver)*bg_primary_sun_transmittance(input.world_position);
    let color = input.color + input.surface_color * local_light - input.direct * (1.0 - visibility);
    // Mark local-source influence reactive before a moving shadow reaches it.
    let history_sign = bg_local_history_sign(input.world_position, normalize(input.normal), input.local_radiance, input.local_direction);
    return bg_scene_output(color,input.indirect,input.world_position,input.sky,history_sign);
}

struct MotionOutput { @builtin(position) @invariant clip: vec4f, @location(0) previous: vec4f, @location(1) current: vec4f };
@vertex fn vs_motion(input: VertexInput, @builtin(instance_index) instance: u32) -> MotionOutput {
    let current = avatar_vertex(input, false);
    let old = previous_world[instance];
    var previous_input = input;
    previous_input.origin = old[0].xyz; previous_input.pose = old[1]; previous_input.orientation = old[2];
    var out: MotionOutput; out.clip = current.clip; out.current = current.clip;
    // The previous pose is evaluated with the same procedural deformation as
    // this frame (feet, squash, ear sway and rigid projectile quaternion).
    if old[0].w == 0.0 { out.previous = vec4f(0.0); }
    else { out.previous = motion_frame.previous * vec4f(avatar_vertex(previous_input, false).world_position, 1.0); }
    return out;
}
@fragment fn fs_motion(input: MotionOutput) -> @location(0) vec4f {
    return bg_encode_motion(input.previous, input.current);
}
