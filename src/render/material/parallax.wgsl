// Explicit original gradients keep the ray march and all three maps at a
// consistent mip, even across a nonuniform height intersection.
struct MaterialCoordinates { uv: vec2f, dx: vec2f, dy: vec2f };

fn bg_parallax_trace(uv: vec2f, layer: i32, dx: vec2f, dy: vec2f,
    ray: vec2f, steps: u32) -> vec2f {
    let step_depth = 1.0 / f32(steps);
    let step_uv = ray * step_depth;
    var current_uv = uv;
    var depth = 0.0;
    var gap = 1.0 - textureSampleGrad(material_normal, material_sampler, current_uv, layer, dx, dy).a;
    if gap <= 0.0 { return uv; }
    for (var i = 0u; i < 64u; i += 1u) {
        let previous_uv = current_uv;
        let previous_gap = gap;
        current_uv -= step_uv;
        depth += step_depth;
        gap = 1.0 - textureSampleGrad(material_normal, material_sampler, current_uv, layer, dx, dy).a - depth;
        if gap <= 0.0 {
            let weight = clamp(previous_gap / max(previous_gap-gap, 0.00001), 0.0, 1.0);
            return mix(previous_uv, current_uv, weight);
        }
        if i + 1u >= steps { break; }
    }
    return current_uv;
}

// March from the visible recessed surface toward the light. Only direct sun
// is occluded; material relief must not suppress unrelated ambient/emission.
fn bg_parallax_shadow(uv: vec2f, layer: i32, dx: vec2f, dy: vec2f,
    light: vec3f, scale: f32) -> f32 {
    if scale <= 0.0 { return 1.0; }
    if light.z <= 0.0 { return 0.0; }
    let height = textureSampleGrad(material_normal, material_sampler, uv, layer, dx, dy).a;
    let depth = 1.0-height;
    if depth <= 0.004 { return 1.0; }
    let ray = light.xy / max(light.z, 0.10) * scale * depth;
    var visibility = 1.0;
    for (var i = 1u; i <= 8u; i += 1u) {
        let t = f32(i) / 8.0;
        let sampled = textureSampleGrad(material_normal, material_sampler, uv+ray*t, layer, dx, dy).a;
        let ray_height = height + depth*t;
        // Small height-domain bias rejects quantization/self-intersection;
        // fractional occlusion softens the eight-step visibility boundary.
        visibility = min(visibility, clamp(1.0-(sampled-ray_height-0.006)*8.0/t,0.0,1.0));
    }
    return visibility;
}

fn bg_material_coordinates(input: VertexOutput, parallax: bool) -> MaterialCoordinates {
    let dx = dpdx(input.uv);
    let dy = dpdy(input.uv);
    let px = dpdx(input.world_position);
    let py = dpdy(input.world_position);
    let unchanged = MaterialCoordinates(input.uv, dx, dy);
    if BG_BSL_REFERENCE || !parallax || camera.parallax.x <= 0.0 || (material_map_flags[u32(input.layer)].flags & 33u) != 33u
        || abs(dx.x * dy.y - dx.y * dy.x) < 0.000000000001
        || !bg_normal_frame_supported(input.normal,px,py) { return unchanged; }
    let eye = camera.eye.xyz - input.world_position;
    let distance = length(eye);
    let frame = bg_texture_frame(input.normal, px, py, dx, dy);
    let view = transpose(frame) * (eye / max(distance, 0.0001));
    let tile_size = f32(textureDimensions(material_normal, 0).x);
    let mip = log2(max(1.0, max(length(dx), length(dy)) * tile_size));
    let ray = bg_parallax_ray(view, distance, mip, camera.parallax);
    if dot(ray, ray) < 0.000000000001 { return unchanged; }
    let steps = u32(mix(camera.parallax.z, 12.0, clamp(view.z, 0.0, 1.0)));
    return MaterialCoordinates(bg_parallax_trace(input.uv, bg_material_layer(input.layer), dx, dy, ray, steps), dx, dy);
}

fn bg_material_relief_visibility(input: VertexOutput, coordinates: MaterialCoordinates) -> f32 {
    let px = dpdx(input.world_position);
    let py = dpdy(input.world_position);
    if BG_BSL_REFERENCE || camera.parallax.x <= 0.0 || (material_map_flags[u32(input.layer)].flags & 33u) != 33u
        || abs(coordinates.dx.x*coordinates.dy.y-coordinates.dx.y*coordinates.dy.x) < 0.000000000001
        || !bg_normal_frame_supported(input.normal,px,py) { return 1.0; }
    let frame = bg_texture_frame(input.normal,px,py,coordinates.dx,coordinates.dy);
    let eye = camera.eye.xyz-input.world_position;
    let distance = length(eye);
    let view = transpose(frame)*(eye/max(distance,0.0001));
    let tile_size = f32(textureDimensions(material_normal,0).x);
    let mip = log2(max(1.0,max(length(coordinates.dx),length(coordinates.dy))*tile_size));
    let scale = bg_parallax_scale(view,distance,mip,camera.parallax);
    let visibility = bg_parallax_shadow(coordinates.uv,bg_material_layer(input.layer),coordinates.dx,coordinates.dy,
        transpose(frame)*normalize(camera.sun.xyz),scale);
    return mix(1.0,visibility,scale/max(camera.parallax.x,0.000001));
}
