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
    for (var i = 0u; i < 32u; i += 1u) {
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

fn bg_material_coordinates(input: VertexOutput, parallax: bool) -> MaterialCoordinates {
    let dx = dpdx(input.uv);
    let dy = dpdy(input.uv);
    let px = dpdx(input.world_position);
    let py = dpdy(input.world_position);
    let unchanged = MaterialCoordinates(input.uv, dx, dy);
    if !parallax || (material_map_flags[u32(input.layer)] & 1u) == 0u
        || abs(dx.x * dy.y - dx.y * dy.x) < 0.000000000001 { return unchanged; }
    let eye = camera.eye.xyz - input.world_position;
    let distance = length(eye);
    let frame = bg_texture_frame(input.normal, px, py, dx, dy);
    let view = transpose(frame) * (eye / max(distance, 0.0001));
    let mip = log2(max(1.0, max(length(dx), length(dy)) * 128.0));
    let ray = bg_parallax_ray(view, distance, mip);
    if dot(ray, ray) < 0.000000000001 { return unchanged; }
    let steps = u32(mix(32.0, 12.0, clamp(view.z, 0.0, 1.0)));
    return MaterialCoordinates(bg_parallax_trace(input.uv, input.layer, dx, dy, ray, steps), dx, dy);
}
