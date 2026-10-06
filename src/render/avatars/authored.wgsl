struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f, cloud: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> joints: array<mat4x4f>;
struct Part { color: vec4f, flags: vec4u };
@group(1) @binding(1) var<storage, read> parts: array<Part>;
@group(2) @binding(0) var albedo: texture_2d<f32>;
@group(2) @binding(1) var pixels: sampler;
struct Material { base: vec4f, alpha: vec4f };
@group(2) @binding(2) var<uniform> material: Material;
struct Input {
    @location(0) position: vec3f, @location(1) normal: vec3f,
    @location(2) uv: vec2f, @location(3) joints: vec4u, @location(4) weights: vec4f,
    @location(5) part: u32, @location(6) origin: vec3f, @location(7) yaw_scale: vec2f,
    @location(8) light_levels: vec2u, @location(9) bounce: vec4u,
    @location(10) glow_bounce: vec4u, @location(11) tint: vec3f,
    @location(12) offsets: vec2u, @location(13) first_person_offset: vec3f,
};
struct Output {
    @builtin(position) @invariant clip: vec4f, @location(0) world: vec3f,
    @location(1) normal: vec3f, @location(2) uv: vec2f,
    @location(3) @interpolate(flat) part: u32,
    @location(4) @interpolate(flat) light_levels: vec2u,
    @location(5) @interpolate(flat) bounce: vec4u,
    @location(6) @interpolate(flat) glow_bounce: vec4u,
    @location(7) @interpolate(flat) tint: vec3f,
};
fn vertex(input: Input, shadow: bool) -> Output {
    let j = input.joints + vec4u(input.offsets.x);
    let transform = joints[j.x]*input.weights.x + joints[j.y]*input.weights.y
        + joints[j.z]*input.weights.z + joints[j.w]*input.weights.w;
    var local = (transform * vec4f(input.position,1.0)).xyz * input.yaw_scale.y;
    if !shadow { local += input.first_person_offset; }
    // Cofactors implement the inverse transpose for nonuniform animated scales.
    let a = transform[0].xyz; let b = transform[1].xyz; let c = transform[2].xyz;
    let determinant = dot(a,cross(b,c));
    let n = cross(b,c)*input.normal.x + cross(c,a)*input.normal.y + cross(a,b)*input.normal.z;
    var normal = input.normal;
    if dot(n,n) > 0.000000000001 { normal = normalize(n * select(1.0,-1.0,determinant < 0.0)); }
    let co = cos(input.yaw_scale.x); let si = sin(input.yaw_scale.x);
    var out: Output;
    out.world = vec3f(local.x*co+local.z*si, local.y, local.z*co-local.x*si) + input.origin;
    out.normal = vec3f(normal.x*co+normal.z*si,normal.y,normal.z*co-normal.x*si);
    out.clip = camera.view_projection * vec4f(out.world,1.0);
    if shadow { out.clip = bg_shadow_project(out.world); }
    out.uv=input.uv; out.part=input.offsets.y+input.part;
    out.light_levels=input.light_levels; out.bounce=input.bounce;
    out.glow_bounce=input.glow_bounce; out.tint=input.tint;
    return out;
}
@vertex fn vs_main(input: Input) -> Output { return vertex(input,false); }
@vertex fn vs_shadow(input: Input) -> Output { return vertex(input,true); }
fn color(input: Output) -> vec4f {
    let part = parts[input.part];
    if part.flags.x == 0u { discard; }
    let sample = textureSampleLevel(albedo,pixels,input.uv,0.0) * material.base;
    if material.alpha.x >= 0.0 && sample.a < material.alpha.x { discard; }
    return vec4f(select(sample.rgb*part.color.rgb,part.color.rgb,part.color.w>0.5),1.0);
}
@fragment fn fs_main(input: Output, @builtin(front_facing) front: bool) -> BgSceneOutput {
    if parts[input.part].flags.y != 0u { discard; }
    let receiver=bg_shadow_receiver(input.world);
    let rgb=color(input).rgb;
    let normal = normalize(select(-input.normal,input.normal,front));
    let sky=f32(input.light_levels.x & 255u)/15.0; let glow=f32((input.light_levels.x >> 8u) & 255u)/15.0;
    let packed_color = vec3f(unpack4x8unorm(input.light_levels.x).zw, unpack4x8unorm(input.light_levels.y).x);
    let local_light = bg_shadowed_local_light(input.world, normal, packed_color * glow * glow, unpack4x8snorm(input.light_levels.y).yzw);
    let light=bg_surface_light(normal,camera.sun,sky,local_light,vec3f(input.bounce.xyz)/255.0,vec3f(input.glow_bounce.xyz)/255.0,1.0);
    let direct=bg_direct_light(normal,camera.sun,sky);
    let visibility=bg_sun_visibility_material(receiver,normal,sky,0.0)*bg_primary_sun_transmittance(input.world);
    let shaded=(light-direct*(1.0-visibility))*input.tint;
    // Mark local-source influence reactive before a moving shadow reaches it.
    let history_sign = bg_local_history_sign(input.world, normal, packed_color * glow * glow, unpack4x8snorm(input.light_levels.y).yzw);
    return bg_scene_output(rgb*shaded,rgb*input.tint*bg_indirect_light(normal,camera.sun,sky,vec3f(input.bounce.xyz)/255.0,vec3f(input.glow_bounce.xyz)/255.0),input.world,sky,history_sign);
}
@fragment fn fs_shadow(input: Output) { _=color(input); }

struct MotionOutput {
    @builtin(position) @invariant clip: vec4f, @location(0) previous: vec4f,
    @location(1) uv: vec2f, @location(2) @interpolate(flat) part: u32,
    @location(3) current: vec4f,
};
@vertex fn vs_motion(input: Input) -> MotionOutput {
    let current = vertex(input, false);
    let j = input.joints + vec4u(input.offsets.x);
    let previous = previous_world[j.x]*input.weights.x + previous_world[j.y]*input.weights.y
        + previous_world[j.z]*input.weights.z + previous_world[j.w]*input.weights.w;
    var out: MotionOutput;
    out.clip = current.clip; out.current = current.clip; out.uv = current.uv; out.part = current.part;
    out.previous = motion_frame.previous * previous * vec4f(input.position, 1.0);
    return out;
}
@fragment fn fs_motion(input: MotionOutput) -> @location(0) vec4f {
    if parts[input.part].flags.y != 0u { discard; }
    var surface: Output; surface.uv = input.uv; surface.part = input.part;
    _ = color(surface);
    return bg_encode_motion(input.previous, input.current);
}
