struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f };
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
    @location(8) light_levels: vec4u, @location(9) bounce: vec4u,
    @location(10) glow_bounce: vec4u, @location(11) tint: vec3f,
    @location(12) offsets: vec2u, @location(13) first_person_offset: vec3f,
};
struct Output {
    @builtin(position) clip: vec4f, @location(0) world: vec3f,
    @location(1) normal: vec3f, @location(2) uv: vec2f,
    @location(3) @interpolate(flat) part: u32,
    @location(4) @interpolate(flat) light_levels: vec4u,
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
    if shadow { out.clip = bg_shadow.view_projection * vec4f(out.world,1.0); }
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
@fragment fn fs_main(input: Output, @builtin(front_facing) front: bool) -> @location(0) vec4f {
    if parts[input.part].flags.y != 0u { discard; }
    let receiver=bg_shadow_receiver(input.world);
    let rgb=color(input).rgb;
    let normal = normalize(select(-input.normal,input.normal,front));
    let sky=f32(input.light_levels.x)/15.0; let glow=f32(input.light_levels.y)/15.0;
    let light=bg_surface_light(normal,camera.sun,sky,glow,vec3f(input.bounce.xyz)/255.0,vec3f(input.glow_bounce.xyz)/255.0,1.0);
    let direct=bg_direct_light(normal,camera.sun,sky);
    let shaded=(light-direct*(1.0-bg_sun_visibility(receiver)))*input.tint;
    return vec4f(bg_apply_fog(rgb*shaded,input.world,sky),1.0);
}
@fragment fn fs_shadow(input: Output) { _=color(input); }
