struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
struct Tile { relative: vec4f, origin: vec4i };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage,read> coverage: array<vec4i>;
@group(0) @binding(2) var<uniform> options: vec4f;
@group(1) @binding(0) var<uniform> tile: Tile;
@group(2) @binding(0) var material: texture_2d_array<f32>;
@group(2) @binding(1) var material_sampler: sampler;
struct MaterialMetadata { flags: u32, layer: u32 };
@group(2) @binding(5) var<storage, read> material_metadata: array<MaterialMetadata>;
struct In { @location(0) position: vec3f, @location(1) color: vec4f, @location(2) surface: u32 };
struct Out {
    @builtin(position) position: vec4f,
    @location(0) relative: vec3f,
    @location(1) local: vec3f,
    @location(2) color: vec4f,
    @location(3) sky: f32,
    @location(4) normal: vec3f,
    @location(5) indirect: vec3f,
    @location(6) @interpolate(flat) surface: u32,
    @location(7) light: vec3f,
};
@vertex fn vs_main(v: In) -> Out {
    var normal = vec3f(0.0);
    normal[(v.surface & 7u) / 2u] = select(-1.0, 1.0, (v.surface & 1u) != 0u);
    let sky = f32((v.surface >> 3u) & 15u) / 15.0;
    let glow = f32((v.surface >> 7u) & 15u) / 15.0;
    var o: Out;
    o.relative = v.position+tile.relative.xyz;
    o.position = camera.view_projection*vec4f(o.relative, 1.0);
    o.local = v.position;
    o.normal = normal;
    o.sky = sky;
    o.surface = v.surface;
    o.color = v.color;
    o.light = bg_surface_light(normal, camera.sun, sky, glow*glow*vec3f(1.0,0.57,0.23), vec3f(0.0), vec3f(0.0), 1.0);
    o.indirect = bg_indirect_daylight(normal, camera.sun, sky);
    return o;
}
fn bg_lod_coverage(v: Out) {
    // Ready near coverage includes known empty chunks, in all three dimensions.
    let world = vec3i(floor(v.local-v.normal*0.002))+tile.origin.xyz;
    let k = world >> vec3u(4u);
    var index = (u32(k.x)*73856093u ^ u32(k.y)*19349663u ^ u32(k.z)*83492791u)&16383u;
    for(var probe=0u; probe<16384u; probe++) {
        let c = coverage[index];
        if c.w == 0 { break; }
        if all(c.xyz == k) { discard; }
        index = (index+1u)&16383u;
    }
}
@fragment fn fs_main(v: Out) -> BgSceneOutput {
    bg_lod_coverage(v);
    let axis = (v.surface & 7u)/2u;
    var uv = vec2f(v.local.x, -v.local.y);
    if axis == 0u { uv = vec2f(v.local.z, -v.local.y); }
    if axis == 1u { uv = vec2f(v.local.z, v.local.x); }
    let dx = dpdx(uv);
    let dy = dpdy(uv);
    let encoded_layer = v.surface >> 13u;
    var albedo = v.color.rgb;
    if encoded_layer != 0u && options.y > 0.5 {
        let texel = textureSampleGrad(material, material_sampler, uv, i32(material_metadata[encoded_layer-1u].layer), dx, dy);
        // Continuous fade to linear texture averages as texels become distant.
        let detail = 1.0-smoothstep(96.0, 240.0, length(v.relative));
        albedo = mix(albedo, texel.rgb, detail);
        if (v.surface & 4096u) != 0u && mix(1.0, texel.a, detail) < 0.5 { discard; }
    }
    return bg_scene_output(albedo*v.light, albedo*v.indirect, v.relative, max(v.sky,camera.eye.w), 1.0);
}
@fragment fn fs_water(v: Out, @builtin(front_facing) front: bool) -> BgSceneOutput {
    let receiver = bg_shadow_receiver(v.local+vec3f(tile.origin.xyz));
    bg_lod_coverage(v);
    let glow = f32((v.surface >> 7u)&15u)/15.0;
    return bg_water_surface(v.color, v.normal, v.sky, glow, v.local+vec3f(tile.origin.xyz), v.relative, front, options.x, bg_sun_visibility(receiver));
}
