struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> joints: array<mat4x4<f32>>;
@group(1) @binding(1) var body: texture_2d<f32>;
@group(1) @binding(2) var hair: texture_2d<f32>;
@group(1) @binding(3) var pixels: sampler;
struct Input {
    @location(0) local: vec3f, @location(1) normal: vec3f, @location(2) joint: u32,
    @location(3) origin: vec3f, @location(4) cosmetics: vec4u,
    @location(5) light_levels: vec4u, @location(6) bounce: vec4u,
    @location(7) pose: vec4f, @location(8) uv: vec2f,
    @location(9) tint: vec3f, @location(10) glow_bounce: vec4u,
    @location(11) material: u32, @builtin(instance_index) instance: u32,
};
struct Output {
    @builtin(position) clip: vec4f, @location(0) light: vec3f,
    @location(1) distance: f32, @location(2) sky: f32,
    @location(3) uv: vec2f, @location(4) @interpolate(flat) material: u32,
};
@vertex fn vs_main(input: Input) -> Output {
    var output: Output;
    let transform = joints[input.instance * 7u + input.joint];
    let local = (transform * vec4f(input.local, 1.0)).xyz;
    let n = normalize((transform * vec4f(input.normal, 0.0)).xyz);
    let c = cos(input.pose.x); let s = sin(input.pose.x);
    let world = vec3f(local.x*c+local.z*s, local.y, local.z*c-local.x*s) + input.origin;
    let normal = vec3f(n.x*c+n.z*s, n.y, n.z*c-n.x*s);
    output.clip = camera.view_projection * vec4f(world, 1.0);
    let sky = f32(input.light_levels.x) / 15.0;
    let glow = f32(input.light_levels.y) / 15.0;
    let bounce = vec3f(input.bounce.xyz) / 255.0;
    let glow_bounce = vec3f(input.glow_bounce.xyz) / 255.0;
    let sun = max(dot(normal, normalize(camera.sun.xyz)), 0.0);
    output.light = input.tint * (vec3f(0.012,0.015,0.022)
        + sky * camera.sun.w * (vec3f(0.31,0.40,0.53) + sun * vec3f(0.77,0.66,0.47))
        + glow * glow * vec3f(1.0,0.57,0.23)
        + mix(glow_bounce,bounce,camera.sun.w)*1.35);
    output.distance = output.clip.w; output.sky = sky;
    output.uv = input.uv; output.material = input.material;
    return output;
}
@fragment fn fs_main(input: Output) -> @location(0) vec4f {
    let body_color = textureSample(body, pixels, input.uv);
    let hair_color = textureSample(hair, pixels, input.uv);
    let albedo = select(body_color,hair_color,input.material == 1u);
    if albedo.a < 0.5 { discard; }
    let fog = smoothstep(38.0,135.0,input.distance);
    let fog_sky = mix(vec3f(0.006,0.009,0.016),camera.horizon.xyz,input.sky);
    return vec4f(mix(albedo.rgb*input.light,fog_sky,fog),1.0);
}
