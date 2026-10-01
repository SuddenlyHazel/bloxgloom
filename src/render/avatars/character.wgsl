struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> joints: array<mat4x4<f32>>;
@group(1) @binding(1) var body: texture_2d<f32>;
@group(1) @binding(3) var pixels: sampler;
@group(1) @binding(4) var hair: texture_2d_array<f32>;
@group(1) @binding(5) var face_features: texture_2d_array<f32>;
@group(1) @binding(6) var iris_masks: texture_2d_array<f32>;
struct Input {
    @location(0) local: vec3f, @location(1) normal: vec3f, @location(2) joint: u32,
    @location(3) origin: vec3f, @location(4) cosmetics: vec4u,
    @location(5) light_levels: vec4u, @location(6) bounce: vec4u,
    @location(7) pose: vec4f, @location(8) uv: vec2f,
    @location(9) tint: vec3f, @location(10) glow_bounce: vec4u,
    @location(11) material: u32, @builtin(instance_index) instance: u32,
    @location(12) recipe: vec4u, @location(13) iris: vec4u,
};
struct Output {
    @builtin(position) clip: vec4f, @location(0) light: vec3f,
    @location(1) distance: f32, @location(2) sky: f32,
    @location(3) uv: vec2f, @location(4) @interpolate(flat) material: u32,
    @location(5) @interpolate(flat) recipe: vec3u,
    @location(6) @interpolate(flat) iris: vec4u,
    @location(7) @interpolate(flat) face: u32,
    @location(8) face_uv: vec2f,
};
@vertex fn vs_main(input: Input) -> Output {
    var output: Output;
    let transform = joints[input.instance * 7u + input.joint];
    let local = (transform * vec4f(input.local, 1.0)).xyz;
    let n = normalize((transform * vec4f(input.normal, 0.0)).xyz);
    let c = cos(input.pose.x); let s = sin(input.pose.x);
    let world = vec3f(local.x*c+local.z*s, local.y, local.z*c-local.x*s) + input.origin;
    let normal = vec3f(n.x*c+n.z*s, n.y, n.z*c-n.x*s);
    output.clip = camera.view_projection * vec4f(world, 1.0);
    let sky = f32(input.light_levels.x) / 15.0;
    let glow = f32(input.light_levels.y) / 15.0;
    let bounce = vec3f(input.bounce.xyz) / 255.0;
    let glow_bounce = vec3f(input.glow_bounce.xyz) / 255.0;
    let sun = max(dot(normal, normalize(camera.sun.xyz)), 0.0);
    output.light = input.tint * (vec3f(0.012,0.015,0.022)
        + sky * camera.sun.w * (vec3f(0.31,0.40,0.53) + sun * vec3f(0.77,0.66,0.47))
        + glow * glow * vec3f(1.0,0.57,0.23)
        + mix(glow_bounce,bounce,camera.sun.w)*1.35);
    output.distance = output.clip.w; output.sky = sky;
    output.uv = input.uv; output.material = input.material;
    output.recipe = vec3u(min(input.recipe.x,7u),min(input.recipe.y,5u),input.recipe.z);
    output.iris = input.iris;
    output.face = select(0u,1u,input.material == 0u && input.joint == 1u && input.normal.z < -0.9);
    output.face_uv = (input.uv * vec2f(512.0,256.0) - vec2f(32.0)) / 32.0;
    return output;
}
// Iris shading is authored in byte-encoded sRGB. Decode only after tint math.
fn srgb_to_linear(rgb: vec3f) -> vec3f {
    return select(pow((rgb+vec3f(0.055))/1.055,vec3f(2.4)),rgb/12.92,rgb<=vec3f(0.04045));
}
fn tint_iris(base: vec3f, shade: f32) -> vec3f {
    let tinted = select(base + (vec3f(255.0)-base)*(shade-128.0)/127.0, base*shade/128.0, shade<=128.0);
    return srgb_to_linear(floor(tinted+vec3f(0.5))/255.0);
}
@fragment fn fs_main(input: Output) -> @location(0) vec4f {
    if input.material != 0u && input.material != input.recipe.z { discard; }
    var albedo = textureSampleLevel(body,pixels,input.uv,0.0);
    if input.material > 0u { albedo = textureSampleLevel(hair,pixels,input.uv,i32(input.material-1u),0.0); }
    if input.face == 1u {
        let clean = textureSampleLevel(face_features,pixels,input.face_uv,0,0.0);
        var eyes = textureSampleLevel(face_features,pixels,input.face_uv,i32(input.recipe.x+1u),0.0);
        let mouth = textureSampleLevel(face_features,pixels,input.face_uv,i32(input.recipe.y+9u),0.0);
        let mask = textureSampleLevel(iris_masks,pixels,input.face_uv,i32(input.recipe.x),0.0);
        if input.iris.w != 0u && mask.a > 0.5 {
            let shade = round(mask.r * 255.0);
            let base = vec3f(input.iris.xyz);
            eyes = vec4f(tint_iris(base,shade),eyes.a);
        }
        let face = mix(clean.rgb,eyes.rgb,eyes.a);
        albedo = vec4f(mix(face,mouth.rgb,mouth.a),1.0);
    }
    if albedo.a < 0.5 { discard; }
    let fog = smoothstep(38.0,135.0,input.distance);
    let fog_sky = mix(vec3f(0.006,0.009,0.016),camera.horizon.xyz,input.sky);
    return vec4f(mix(albedo.rgb*input.light,fog_sky,fog),1.0);
}
