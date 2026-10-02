struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> joints: array<mat4x4<f32>>;
@group(1) @binding(1) var body: texture_2d_array<f32>;
@group(1) @binding(3) var pixels: sampler;
@group(1) @binding(4) var hair: texture_2d_array<f32>;
@group(1) @binding(5) var face_features: texture_2d_array<f32>;
@group(1) @binding(6) var iris_masks: texture_2d_array<f32>;
// REGISTERED_PALETTES
struct Input {
    @location(0) local: vec3f, @location(1) normal: vec3f, @location(2) joint: u32,
    @location(3) origin: vec3f, @location(4) cosmetics: vec4u,
    @location(5) light_levels: vec4u, @location(6) bounce: vec4u,
    @location(7) pose: vec4f, @location(8) uv: vec2f,
    @location(9) tint: vec3f, @location(10) glow_bounce: vec4u,
    @location(11) material: u32, @builtin(instance_index) instance: u32,
    @location(12) recipe: vec4u, @location(13) iris: vec4u,
    @location(14) hair_color_body: vec4u, @location(15) surface: u32,
};
struct Output {
    @builtin(position) clip: vec4f, @location(0) light: vec3f,
    @location(2) sky: f32,
    @location(3) uv: vec2f, @location(4) @interpolate(flat) material: u32,
    @location(5) @interpolate(flat) recipe: vec3u,
    @location(6) @interpolate(flat) iris: vec4u,
    @location(7) @interpolate(flat) surface: u32,
    @location(8) local: vec3f,
    @location(9) local_height: f32,
    @location(10) @interpolate(flat) eye_height: f32,
    @location(11) @interpolate(flat) joint: u32,
    @location(12) @interpolate(flat) hair_color_body: vec4u,
    @location(13) @interpolate(flat) cosmetics: vec3u,
    @location(14) front: f32,
    @location(15) world_position: vec3f,
};
@vertex fn vs_main(input: Input) -> Output {
    var output: Output;
    let transform = joints[input.instance * 30u + input.joint];
    let local = (transform * vec4f(input.local, 1.0)).xyz;
    let n = normalize((transform * vec4f(input.normal, 0.0)).xyz);
    let c = cos(input.pose.x); let s = sin(input.pose.x);
    let world = vec3f(local.x*c+local.z*s, local.y, local.z*c-local.x*s) + input.origin;
    let normal = vec3f(n.x*c+n.z*s, n.y, n.z*c-n.x*s);
    output.clip = camera.view_projection * vec4f(world, 1.0);
    let head = input.joint == 5u || (input.joint >= 15u && input.joint <= 17u) || input.joint >= 27u;
    if input.recipe.w != 0u && head { output.clip = vec4f(2.0, 2.0, 2.0, 1.0); }
    // Styled eye artwork uses the articulated white box as its canvas. Hide the
    // native iris, pupil and glint geometry only when that canvas is selected.
    if input.recipe.x != 0u && (input.surface == 4u || input.surface == 5u || input.surface == 6u || (input.surface == 2u && head)) {
        output.clip = vec4f(2.0, 2.0, 2.0, 1.0);
    }
    output.local_height = local.y;
    output.eye_height = select(0.0, input.pose.w, input.recipe.w != 0u);
    output.joint = input.joint;
    let sky = f32(input.light_levels.x) / 15.0;
    let glow = f32(input.light_levels.y) / 15.0;
    let bounce = vec3f(input.bounce.xyz) / 255.0;
    let glow_bounce = vec3f(input.glow_bounce.xyz) / 255.0;
    let sun = max(dot(normal, normalize(camera.sun.xyz)), 0.0);
    output.light = input.tint * (vec3f(0.012,0.015,0.022)
        + sky * camera.sun.w * (vec3f(0.31,0.40,0.53) + sun * vec3f(0.77,0.66,0.47))
        + glow * glow * vec3f(1.0,0.57,0.23)
        + mix(glow_bounce,bounce,camera.sun.w)*1.35);
    output.world_position = world;
    output.sky = sky;
    output.uv = input.uv; output.material = input.material;
    output.recipe = vec3u(min(input.recipe.x,7u),min(input.recipe.y,5u),input.recipe.z);
    output.iris = input.iris; output.surface = input.surface; output.local = input.local;
    output.front = input.normal.z;
    output.hair_color_body = input.hair_color_body; output.cosmetics = input.cosmetics.xyz;
    return output;
}
fn srgb_to_linear(rgb: vec3f) -> vec3f {
    return select(pow((rgb+vec3f(0.055))/1.055,vec3f(2.4)),rgb/12.92,rgb<=vec3f(0.04045));
}
fn tint_iris(base: vec3f, shade: f32) -> vec3f {
    let tinted = select(base + (vec3f(255.0)-base)*(shade-128.0)/127.0, base*shade/128.0, shade<=128.0);
    return srgb_to_linear(floor(tinted+vec3f(0.5))/255.0);
}
// The texture is already decoded by its sRGB texture format. Decode the user
// RGB exactly once, multiply in linear light. Accessory primitives never tint.
fn tint_hair(neutral: vec3f, rgb: vec3f) -> vec3f { return neutral * srgb_to_linear(rgb / 255.0); }
fn shade_hair(neutral: vec4f, rgb: vec3f, fixed: bool) -> vec4f {
    return vec4f(select(tint_hair(neutral.rgb,rgb),neutral.rgb,fixed),neutral.a);
}
@fragment fn fs_main(input: Output) -> @location(0) vec4f {
    if input.eye_height > 0.0 && (input.joint == 3u || input.joint == 4u) && input.local_height > input.eye_height - 0.12 { discard; }
    let is_hair = input.material > 0u && input.material < 14u;
    if is_hair && input.material != input.recipe.z { discard; }
    var albedo = textureSampleLevel(body,pixels,input.uv,i32(input.hair_color_body.w),0.0);
    if is_hair {
        let fixed = select(0u,1u,input.surface == 8u);
        albedo = textureSampleLevel(hair,pixels,input.uv,i32((input.material-1u)*2u+fixed),0.0);
        albedo = shade_hair(albedo,vec3f(input.hair_color_body.xyz),fixed != 0u);
    } else {
        if input.surface == 0u {
            // Preserve the painted shade ratios; palette entries remain the
            // same startup-validated colors used by the rest of the game.
            let shade = clamp(dot(albedo.rgb, vec3f(0.2126,0.7152,0.0722))/0.425405,0.0,1.0);
            albedo = vec4f(shade * SKINS[min(input.cosmetics.x,31u)],albedo.a);
        }
        if input.surface == 1u && input.cosmetics.z != 0u { albedo = vec4f(clamp(dot(albedo.rgb,vec3f(0.2126,0.7152,0.0722))/0.08,0.0,1.0) * PANTS[min(input.cosmetics.z,31u)],albedo.a); }
        if input.surface == 9u && input.cosmetics.y != 0u { albedo = vec4f(clamp(dot(albedo.rgb,vec3f(0.2126,0.7152,0.0722))/0.08,0.0,1.0) * SHIRTS[min(input.cosmetics.y,31u)],albedo.a); }
        if input.surface == 4u && input.iris.w != 0u { albedo = vec4f(srgb_to_linear(vec3f(input.iris.xyz)/255.0),albedo.a); }
        if input.surface == 3u && input.recipe.x != 0u {
            if input.front > -0.9 { discard; }
            let side = select(16.0,0.0,input.joint == 15u);
            let eye_uv = (vec2f(side+3.0,8.0) + vec2f(1.0-(input.local.x+0.0535)/0.107,1.0-(input.local.y+0.0595)/0.119)*vec2f(12.0,17.0))/32.0;
            var eye = textureSampleLevel(face_features,pixels,eye_uv,i32(input.recipe.x+1u),0.0);
            let mask = textureSampleLevel(iris_masks,pixels,eye_uv,i32(input.recipe.x),0.0);
            if input.iris.w != 0u && mask.a > 0.5 { eye = vec4f(tint_iris(vec3f(input.iris.xyz),round(mask.r*255.0)),eye.a); }
            albedo = eye;
        }
        if input.joint == 5u && input.surface == 0u && input.front < -0.9 {
            let face_uv = vec2f(1.0-(input.local.x+0.255)/0.51,1.0-(input.local.y+0.008)/0.48);
            let mouth = textureSampleLevel(face_features,pixels,face_uv,i32(input.recipe.y+9u),0.0);
            albedo = vec4f(mix(albedo.rgb,mouth.rgb,mouth.a),albedo.a);
        }
    }
    if albedo.a < 0.5 { discard; }
    return vec4f(bg_apply_fog(albedo.rgb * input.light, input.world_position, input.sky), 1.0);
}
