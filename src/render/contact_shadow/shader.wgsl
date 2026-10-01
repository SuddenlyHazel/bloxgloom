struct Camera {
    view_proj: mat4x4<f32>,
    sun: vec4f,
    horizon: vec4f,
    eye: vec4f,
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct Input {
    @location(0) bounds: vec4f,
    @location(1) center: vec4f,
    @location(2) light: vec4f,
};
struct Output {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) world: vec3f,
    @location(2) light: vec3f,
};
@vertex fn vs(input: Input, @builtin(vertex_index) vertex: u32) -> Output {
    let corners = array<vec2f, 6>(vec2f(0.0, 0.0), vec2f(0.0, 1.0), vec2f(1.0, 0.0),
        vec2f(1.0, 0.0), vec2f(0.0, 1.0), vec2f(1.0, 1.0));
    let xz = mix(input.bounds.xy, input.bounds.zw, corners[vertex]);
    var out: Output;
    out.world = vec3f(xz.x, input.center.z, xz.y);
    out.position = camera.view_proj * vec4f(out.world, 1.0);
    out.uv = (xz - input.center.xy) / input.center.w;
    out.light = input.light.xyz;
    return out;
}
@fragment fn fs(input: Output) -> @location(0) vec4f {
    let radial = 1.0 - smoothstep(0.0, 1.0, dot(input.uv, input.uv));
    let illumination = max(input.light.y * camera.sun.w, input.light.z);
    let lit = smoothstep(0.025, 0.35, illumination);
    let distance = length(input.world - camera.eye.xyz);
    let near = 1.0 - smoothstep(18.0, 28.0, distance);
    // Match weather fog transmission. Never stamp a dark spot over bright fog.
    let exposure = max(camera.eye.w, smoothstep(0.0, 0.1, input.light.y));
    let optical_depth = max(distance - 6.0, 0.0) * camera.horizon.w * exposure;
    let transmission = exp(-pow(optical_depth, 1.5));
    // Black alpha blend only multiplies existing HDR color; it cannot create
    // light in sealed caves or alter day/night color, emission, or voxel fields.
    return vec4f(0.0, 0.0, 0.0, input.light.x * radial * radial * lit * near * transmission);
}
