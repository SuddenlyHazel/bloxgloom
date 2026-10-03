@group(0) @binding(0) var<uniform> camera: mat4x4<f32>;
struct Input {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex fn vs(input: Input) -> Output {
    var out: Output;
    out.position = camera * vec4<f32>(input.position, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}
struct FireOutput { @location(0) color: vec4f, @location(1) indirect: vec4f };
@fragment fn fs(input: Output) -> FireOutput {
    let tip = 1.0 - smoothstep(0.45, 1.0, input.uv.y);
    let alpha = input.color.a * tip;
    // Invisible tips must not overwrite the background's temporal reactivity.
    if alpha == 0.0 { discard; }
    return FireOutput(vec4f(input.color.rgb,alpha),vec4f(0.0,0.0,0.0,-1.0));
}
