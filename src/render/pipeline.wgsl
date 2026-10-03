const BG_MATERIAL_HISTORY_SIGN: f32 = 1.0;

struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) layer: f32,
    @location(4) light_levels: vec2<f32>,
    @location(5) bounce_packed: f32,
    @location(6) glow_bounce_packed: f32,
    @location(7) local_rgb_packed: f32,
    @location(8) local_direction_packed: f32,
};
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: vec3<f32>,
    @location(2) @interpolate(flat) layer: i32,
    @location(3) indirect_bounce: vec4f,
    @location(4) sky_level: f32,
    @location(5) world_position: vec3f,
    @location(6) normal: vec3f,
    @location(7) @interpolate(flat) history_sign: f32,
    @location(8) local_radiance: vec3f,
    @location(9) local_direction: vec3f,
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
    let reactive = input.glow_bounce_packed < 0.0;
    let glow_encoded = u32(select(input.glow_bounce_packed, -input.glow_bounce_packed - 1.0, reactive));
    output.history_sign = select(1.0, -1.0, reactive);
    let glow_bounce = vec3<f32>(f32(glow_encoded & 255u), f32((glow_encoded >> 8u) & 255u), f32((glow_encoded >> 16u) & 255u)) / 255.0;
    output.local_radiance = unpack4x8unorm(u32(input.local_rgb_packed)).xyz * glow;
    output.local_direction = unpack4x8snorm(u32(input.local_direction_packed)).xyz;
    output.light = bg_surface_light(vertex.normal, camera.sun, sky, vec3f(0.0), bounce, glow_bounce, 1.0);
    output.indirect_bounce = vec4f(mix(glow_bounce,bounce,camera.sun.w)*1.35,1.0-2.0*fract(input.layer));
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
struct MaterialSurface { shaded: BgSurface, light: vec3f, direct: vec3f, indirect: vec3f, visibility: f32 };
fn surface(input: VertexOutput, albedo: vec4f, coordinates: MaterialCoordinates) -> MaterialSurface {
    let normal = bg_material_normal(input, coordinates);
    // Compute the card plane before any cutout discard, in both color/caster
    // passes. Keep the established shading normal for diffuse and normal maps.
    let transmission_normal = bg_thin_transmission_normal(input.normal, normal,
        cross(dpdx(input.world_position), dpdy(input.world_position)),
        camera.eye.xyz - input.world_position);
    let direct = bg_foliage_direct(normal, transmission_normal, camera.sun,
        input.sky_level, u32(input.layer));
    let flat = bg_surface_light(input.normal, camera.sun, input.sky_level, vec3f(0.0), vec3f(0.0), vec3f(0.0), 1.0);
    let mapped = bg_surface_light(normal, camera.sun, input.sky_level, vec3f(0.0), vec3f(0.0), vec3f(0.0), 1.0);
    var light = max(vec3f(0.0), input.light + (mapped - flat) + bg_shadowed_local_light(input.world_position, normal, input.local_radiance, input.local_direction));
    if (material_map_flags[u32(input.layer)] & 0x00ffff00u) != 0u {
        light = max(vec3f(0.0), light
            + direct
            - bg_direct_light(normal, camera.sun, input.sky_level));
    }
    // Validated moving-character contacts attenuate sky fill once, before
    // material hooks, direct-sun visibility, emission, specular and fog.
    let contact = bg_scene_contact(input.world_position, input.normal);
    light = bg_contact_light(light, normal, camera.sun, input.sky_level, contact);
    let shaded = bg_surface(BgSurface(albedo, input.world_position, normal,
        coordinates.uv, light, albedo.rgb * material_emission[u32(input.layer)]), u32(input.layer));
    // Normalize the actually retained indirect energy by a conservative common
    // visibility. The product E*visibility exactly equals the current sky-fill
    // plus bounce, including overlapping baked corner and analytic contact AO.
    // This unions scene AO without reapplying either existing suppression.
    let baked = max(input.indirect_bounce.w,0.00001);
    let indirect = bg_local_indirect_record(bg_indirect_daylight(normal,camera.sun,input.sky_level),input.indirect_bounce.xyz,baked,contact);
    let hook_scale = clamp(shaded.light/max(light,vec3f(0.00001)),vec3f(0.0),vec3f(16.0));
    return MaterialSurface(shaded, light, direct, shaded.albedo.rgb*indirect.xyz*hook_scale, indirect.w);
}
fn shade(input: VertexOutput, material_surface: MaterialSurface, specular: vec4f, receiver: BgShadowReceiver) -> BgSceneOutput {
    let surface = material_surface.shaded;
    let sun_visibility = bg_sun_visibility(receiver);
    let highlight = bg_material_highlight(input,surface,specular,sun_visibility,material_surface.visibility);
    let shadowed = max(vec3f(0.0),material_surface.light-material_surface.direct*(1.0-sun_visibility));
    let ratio = clamp(shadowed/max(material_surface.light,vec3f(0.00001)),vec3f(0.0),vec3f(1.0));
    let eye = camera.eye.xyz-input.world_position;
    let nv = dot(normalize(surface.normal),eye/max(length(eye),0.0001));
    let diffuse_weight = bg_material_diffuse_weight(specular,nv);
    let color = surface.albedo.rgb*surface.light*ratio*diffuse_weight+surface.emission+highlight;
    return bg_scene_output(color,material_surface.indirect*diffuse_weight,input.world_position,input.sky_level,
        material_surface.visibility * min(min(BG_MATERIAL_HISTORY_SIGN, input.history_sign), bg_local_history_sign(input.world_position, surface.normal, input.local_radiance, input.local_direction)));
}
@fragment fn fs_main(input: VertexOutput) -> BgSceneOutput {
    let receiver = bg_shadow_receiver(input.world_position);
    let coordinates = bg_material_coordinates(input, true);
    let albedo = textureSampleGrad(material, material_sampler, coordinates.uv, input.layer, coordinates.dx, coordinates.dy);
    return shade(input, surface(input, albedo, coordinates), bg_material_specular(input, coordinates), receiver);
}
@fragment fn fs_cutout(input: VertexOutput) -> BgSceneOutput {
    let receiver = bg_shadow_receiver(input.world_position);
    // Keep camera-dependent height offsets away from cutout alpha and casters.
    let coordinates = bg_material_coordinates(input, false);
    let texel = textureSampleGrad(material, material_sampler, coordinates.uv, input.layer, coordinates.dx, coordinates.dy);
    let shaded = surface(input, texel, coordinates);
    let specular = bg_material_specular(input, coordinates);
    if shaded.shaded.albedo.a < 0.5 { discard; }
    return shade(input, shaded, specular, receiver);
}

fn bg_discard_local_emitter(world: vec3f) {
    // Only this source block is excluded; nearby walls and other emitters
    // remain casters. Sun maps never use the negative local-caster sentinel.
    if bg_shadow.params.z < 0.0 && all(abs(world-bg_shadow.center.xyz) <= vec3f(0.501)) { discard; }
}
@fragment fn fs_shadow_opaque(input: VertexOutput) { bg_discard_local_emitter(input.world_position); }
@fragment fn fs_shadow(input: VertexOutput) {
    let coordinates = bg_material_coordinates(input, false);
    let albedo = textureSampleGrad(material, material_sampler, coordinates.uv, input.layer, coordinates.dx, coordinates.dy);
    let shaded = surface(input, albedo, coordinates);
    bg_discard_local_emitter(input.world_position);
    if shaded.shaded.albedo.a < 0.5 { discard; }
}
