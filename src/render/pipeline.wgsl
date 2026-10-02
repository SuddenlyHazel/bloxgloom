
struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f };
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
    @location(4) sky_level: f32,
    @location(5) world_position: vec3f,
    @location(6) normal: vec3f,
};
@group(1) @binding(0) var material: texture_2d_array<f32>;
@group(1) @binding(1) var material_sampler: sampler;
@group(1) @binding(2) var<storage, read> material_emission: array<f32>;
@group(1) @binding(3) var material_normal: texture_2d_array<f32>;
@group(1) @binding(4) var material_specular: texture_2d_array<f32>;
@group(1) @binding(5) var<storage, read> material_map_flags: array<u32>;
fn voxel_vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let vertex = bg_vertex(BgVertex(input.position, input.normal, input.uv), u32(input.layer));
    output.position = camera.view_projection * vec4<f32>(vertex.position, 1.0);
    let sky = input.light_levels.x;
    let glow = input.light_levels.y;
    let encoded = u32(input.bounce_packed);
    let bounce = vec3<f32>(f32(encoded & 255u), f32((encoded >> 8u) & 255u), f32((encoded >> 16u) & 255u)) / 255.0;
    let glow_encoded = u32(input.glow_bounce_packed);
    let glow_bounce = vec3<f32>(f32(glow_encoded & 255u), f32((glow_encoded >> 8u) & 255u), f32((glow_encoded >> 16u) & 255u)) / 255.0;
    output.light = bg_surface_light(vertex.normal, camera.sun, sky, glow, bounce, glow_bounce, 1.0);
    output.uv = vertex.uv;
    output.layer = i32(input.layer);
    output.sky_level = sky;
    output.world_position = vertex.position;
    output.normal = vertex.normal;
    return output;
}
@vertex fn vs_main(input: VertexInput) -> VertexOutput { return voxel_vertex(input); }
@vertex fn vs_shadow(input: VertexInput) -> VertexOutput {
    var output = voxel_vertex(input);
    output.position = bg_shadow.view_projection * vec4f(output.world_position, 1.0);
    return output;
}
struct MaterialSurface { shaded: BgSurface, light: vec3f, normal: vec3f };
fn surface(input: VertexOutput, albedo: vec4f, coordinates: MaterialCoordinates) -> MaterialSurface {
    let normal = bg_material_normal(input, coordinates);
    let flat = bg_surface_light(input.normal, camera.sun, input.sky_level, 0.0, vec3f(0.0), vec3f(0.0), 1.0);
    let mapped = bg_surface_light(normal, camera.sun, input.sky_level, 0.0, vec3f(0.0), vec3f(0.0), 1.0);
    let light = max(vec3f(0.0), input.light + (mapped - flat));
    let shaded = bg_surface(BgSurface(albedo, input.world_position, normal,
        coordinates.uv, light, albedo.rgb * material_emission[u32(input.layer)]), u32(input.layer));
    return MaterialSurface(shaded, light, normal);
}
fn shade(input: VertexOutput, material_surface: MaterialSurface, specular: vec4f, receiver: BgShadowReceiver) -> vec4<f32> {
    let surface = material_surface.shaded;
    // Hooks see the same unshadowed input in both passes, so light-dependent
    // alpha stays identical. Apply sun visibility to the final lit component;
    // authored emission remains independent of occluders.
    let sun_visibility = bg_sun_visibility(receiver);
    let highlight = bg_material_highlight(input, surface, specular, sun_visibility);
    if sun_visibility >= 1.0 {
        return vec4f(bg_apply_fog(surface.albedo.rgb * surface.light + surface.emission + highlight,
            input.world_position, input.sky_level), 1.0);
    }
    let direct = bg_direct_light(material_surface.normal, camera.sun, input.sky_level);
    let shadowed = max(vec3f(0.0), material_surface.light - direct * (1.0-sun_visibility));
    let visibility = clamp(shadowed / max(material_surface.light, vec3f(0.00001)), vec3f(0.0), vec3f(1.0));
    return vec4f(bg_apply_fog(surface.albedo.rgb * surface.light * visibility + surface.emission + highlight,
        input.world_position, input.sky_level), 1.0);
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let receiver = bg_shadow_receiver(input.world_position);
    let coordinates = bg_material_coordinates(input, true);
    let albedo = textureSampleGrad(material, material_sampler, coordinates.uv, input.layer, coordinates.dx, coordinates.dy);
    return shade(input, surface(input, albedo, coordinates), bg_material_specular(input, coordinates), receiver);
}
@fragment fn fs_cutout(input: VertexOutput) -> @location(0) vec4<f32> {
    let receiver = bg_shadow_receiver(input.world_position);
    // Keep camera-dependent height offsets away from cutout alpha and casters.
    let coordinates = bg_material_coordinates(input, false);
    let texel = textureSampleGrad(material, material_sampler, coordinates.uv, input.layer, coordinates.dx, coordinates.dy);
    let shaded = surface(input, texel, coordinates);
    let specular = bg_material_specular(input, coordinates);
    if shaded.shaded.albedo.a < 0.5 { discard; }
    return shade(input, shaded, specular, receiver);
}

@fragment fn fs_shadow(input: VertexOutput) {
    let coordinates = bg_material_coordinates(input, false);
    let albedo = textureSampleGrad(material, material_sampler, coordinates.uv, input.layer, coordinates.dx, coordinates.dy);
    let shaded = surface(input, albedo, coordinates);
    if shaded.shaded.albedo.a < 0.5 { discard; }
}
