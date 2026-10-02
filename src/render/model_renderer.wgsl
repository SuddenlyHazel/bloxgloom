struct Camera { view_projection: mat4x4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> joints: array<mat4x4f>;
@group(1) @binding(0) var albedo: texture_2d<f32>;
@group(1) @binding(1) var pixels: sampler;
struct Material { base: vec4f, tint: vec4f, alpha: vec4f };
@group(1) @binding(2) var<uniform> material: Material;
struct Input {
    @location(0) position: vec3f, @location(1) normal: vec3f,
    @location(2) uv: vec2f, @location(3) joints: vec4u, @location(4) weights: vec4f,
};
struct Output { @builtin(position) clip: vec4f, @location(0) normal: vec3f, @location(1) uv: vec2f };
@vertex fn vs_main(input: Input) -> Output {
    let transform = joints[input.joints.x]*input.weights.x + joints[input.joints.y]*input.weights.y
        + joints[input.joints.z]*input.weights.z + joints[input.joints.w]*input.weights.w;
    // Inverse-transpose normal transform handles nonuniform node/skin scales.
    let a = transform[0].xyz; let b = transform[1].xyz; let c = transform[2].xyz;
    let determinant = dot(a,cross(b,c));
    let n = (cross(b,c)*input.normal.x + cross(c,a)*input.normal.y + cross(a,b)*input.normal.z);
    var output: Output;
    output.clip = camera.view_projection * transform * vec4f(input.position,1.0);
    var safe_normal = input.normal;
    if dot(n,n) > 0.000000000001 {
        safe_normal = normalize(n * select(1.0,-1.0,determinant < 0.0));
    }
    output.normal = safe_normal;
    output.uv = input.uv;
    return output;
}
@fragment fn fs_main(input: Output, @builtin(front_facing) front: bool) -> @location(0) vec4f {
    let sampled = textureSample(albedo,pixels,input.uv) * material.base;
    if material.alpha.x >= 0.0 && sampled.a < material.alpha.x { discard; }
    let rgb = select(sampled.rgb * material.tint.rgb, material.tint.rgb,material.tint.w > 0.5);
    let normal = normalize(select(-input.normal,input.normal,front));
    let light = 0.35 + 0.65*max(0.0,dot(normal,normalize(vec3f(-0.5,0.8,-0.7))));
    return vec4f(rgb*light,1.0);
}
