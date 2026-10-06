struct Camera { view_projection:mat4x4f,sun:vec4f,horizon:vec4f,eye:vec4f,fog_range:vec4f,parallax:vec4f,sun_radiance:vec4f,sky_zenith:vec4f,ambient_lower:vec4f,ambient_upper:vec4f,cloud:vec4f,ambient_sh:array<vec4f,6> };
@group(0) @binding(0) var<uniform> camera: Camera;
struct Input {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3f,
};
@vertex fn vs(input: Input) -> Output {
    var out: Output;
    out.position = camera.view_projection * vec4<f32>(input.position, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    out.world = input.position;
    return out;
}
struct FireOutput { @location(0) color: vec4f, @location(1) indirect: vec4f };
@fragment fn fs(input: Output) -> FireOutput {
    let tip = 1.0 - smoothstep(0.45, 1.0, input.uv.y);
    // Attenuate particle coverage/emission, not the already-resolved scene.
    // Keeping source RGB preserves emissive fire energy and avoids black
    // cutouts when the medium completely hides a translucent particle.
    let alpha = input.color.a * tip * bg_particle_transmittance(input.world);
    // Invisible tips must not overwrite the background's temporal reactivity.
    if alpha == 0.0 { discard; }
    return FireOutput(vec4f(input.color.rgb,alpha),vec4f(0.0,0.0,0.0,-1.0));
}
