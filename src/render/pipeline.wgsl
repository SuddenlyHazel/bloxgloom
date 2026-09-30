
struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: f32,
    @location(4) light_levels: vec2<f32>,
    @location(5) bounce_packed: f32,
    @location(6) glow_bounce_packed: f32,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
    @location(2) @interpolate(flat) layer: i32,
    @location(3) distance: f32,
    @location(4) sky_level: f32,
    @location(5) world_position: vec3f,
    @location(6) normal: vec3f,
};
@group(1) @binding(0) var material: texture_2d_array<f32>;
@group(1) @binding(1) var material_sampler: sampler;
@group(1) @binding(2) var<storage, read> material_emission: array<f32>;
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let vertex = bg_vertex(BgVertex(input.position, input.normal, input.uv), u32(input.layer));
    output.position = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    let sunlight = max(dot(vertex.normal, normalize(camera.sun.xyz)), 0.0);
    let sky = input.light_levels.x;
    let glow = input.light_levels.y;
    let encoded = u32(input.bounce_packed);
    let bounce = vec3<f32>(f32(encoded & 255u), f32((encoded >> 8u) & 255u), f32((encoded >> 16u) & 255u)) / 255.0;
    let glow_encoded = u32(input.glow_bounce_packed);
    let glow_bounce = vec3<f32>(f32(glow_encoded & 255u), f32((glow_encoded >> 8u) & 255u), f32((glow_encoded >> 16u) & 255u)) / 255.0;
    output.light = vec3<f32>(0.012, 0.015, 0.022)
        + sky * camera.sun.w * (vec3<f32>(0.31, 0.40, 0.53)
            + sunlight * vec3<f32>(0.77, 0.66, 0.47))
        + glow * glow * vec3<f32>(1.0, 0.57, 0.23)
        + mix(glow_bounce, bounce, camera.sun.w) * 1.35;
    output.uv = vertex.uv;
    output.layer = i32(input.layer);
    output.distance = output.position.w;
    output.sky_level = sky;
    output.world_position = vertex.position;
    output.normal = vertex.normal;
    return output;
}
fn surface(input: VertexOutput, albedo: vec4f) -> BgSurface {
    return bg_surface(BgSurface(albedo, input.world_position, input.normal,
        input.uv, input.light, albedo.rgb * material_emission[u32(input.layer)]), u32(input.layer));
}
fn shade(input: VertexOutput, surface: BgSurface) -> vec4<f32> {
    let fog = smoothstep(38.0, 135.0, input.distance);
    let fog_sky = mix(vec3<f32>(0.006, 0.009, 0.016), camera.horizon.xyz, input.sky_level);
    return vec4<f32>(mix(surface.albedo.rgb * surface.light + surface.emission, fog_sky, fog), 1.0);
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let albedo = textureSample(material, material_sampler, input.uv, input.layer);
    return shade(input, surface(input, albedo));
}
@fragment fn fs_cutout(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(material, material_sampler, input.uv, input.layer);
    let shaded = surface(input, texel);
    if shaded.albedo.a < 0.5 { discard; }
    return shade(input, shaded);
}
