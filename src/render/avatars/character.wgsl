struct Camera { view_projection: mat4x4<f32>, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> joints: array<mat4x4<f32>>;
@group(1) @binding(1) var body: texture_2d_array<f32>;
@group(1) @binding(3) var pixels: sampler;
@group(1) @binding(4) var hair: texture_2d_array<f32>;
// REGISTERED_PALETTES
// FIRST_PERSON_JOINT_OFFSET
struct Input {
    @location(0) local: vec3f, @location(1) normal: vec3f, @location(2) joint: u32,
    @location(3) origin: vec3f, @location(4) cosmetics: vec4u,
    @location(5) light_levels: vec2u, @location(6) bounce: vec4u,
    @location(7) pose: vec4f, @location(8) uv: vec2f,
    @location(9) tint: vec3f, @location(10) glow_bounce: vec4u,
    @location(11) material: u32, @builtin(instance_index) instance: u32,
    @location(12) recipe: vec4u, @location(13) iris: vec4u,
    @location(14) hair_color_body: vec4u, @location(15) surface: u32,
};
struct Output {
    @builtin(position) clip: vec4f, @location(0) light: vec4f,
    @location(1) direct: vec4f,
    // Pack local-light inputs into existing slots to retain the 16-varying limit.
    // The spare w components of light/indirect/world_position carry tint.rgb.
    @location(2) normal_sky: vec4f,
    @location(3) uv: vec2f, @location(4) @interpolate(flat) material: u32,
    @location(5) @interpolate(flat) recipe: vec3u,
    @location(6) @interpolate(flat) iris: vec4u,
    @location(7) @interpolate(flat) surface: u32,
    @location(8) indirect: vec4f,
    @location(9) direction_height: vec4f,
    @location(10) @interpolate(flat) radiance_eye_height: vec4f,
    @location(11) @interpolate(flat) joint: u32,
    @location(12) @interpolate(flat) hair_color_body: vec4u,
    @location(13) @interpolate(flat) cosmetics: vec3u,
    @location(14) @interpolate(flat) texture: u32,
    @location(15) world_position: vec4f,
};
fn character_vertex(input: Input, shadow: bool) -> Output {
    var output: Output;
    // The color pass uses the owner's camera-framed rig. Casters always use
    // the unmodified world rig, including the full head and attached arms.
    let first_person = input.recipe.w != 0u && !shadow;
    let offset = select(input.instance * 30u, FIRST_PERSON_JOINT_OFFSET, first_person);
    let transform = joints[offset + input.joint];
    let local = (transform * vec4f(input.local, 1.0)).xyz;
    let n = normalize((transform * vec4f(input.normal, 0.0)).xyz);
    let c = cos(input.pose.x); let s = sin(input.pose.x);
    let world = vec3f(local.x*c+local.z*s, local.y, local.z*c-local.x*s) + input.origin;
    let normal = vec3f(n.x*c+n.z*s, n.y, n.z*c-n.x*s);
    output.clip = camera.view_projection * vec4f(world, 1.0);
    if shadow { output.clip = bg_shadow.view_projection * vec4f(world, 1.0); }
    let surface = input.surface & 255u;
    let head = input.joint == 5u || (input.joint >= 15u && input.joint <= 17u) || input.joint >= 27u;
    if first_person && head { output.clip = vec4f(2.0, 2.0, 2.0, 1.0); }
    output.joint = input.joint;
    let sky = f32(input.light_levels.x & 255u) / 15.0;
    let glow = f32((input.light_levels.x >> 8u) & 255u) / 15.0;
    let packed_color = vec3f(unpack4x8unorm(input.light_levels.x).zw, unpack4x8unorm(input.light_levels.y).x);
    output.direction_height = vec4f(unpack4x8snorm(input.light_levels.y).yzw, local.y);
    output.radiance_eye_height = vec4f(packed_color * glow * glow, select(0.0, input.pose.w, first_person));
    let bounce = vec3f(input.bounce.xyz) / 255.0;
    let glow_bounce = vec3f(input.glow_bounce.xyz) / 255.0;
    let visibility = f32((input.surface >> 8u) & 255u) / 255.0;
    output.light = vec4f(input.tint * bg_surface_light(normal, camera.sun, sky, vec3f(0.0), bounce, glow_bounce, visibility), input.tint.x);
    output.direct = vec4f(input.tint * bg_direct_light(normal,camera.sun,sky),visibility);
    output.indirect = vec4f(input.tint * bg_indirect_light(normal,camera.sun,sky,bounce,glow_bounce), input.tint.y);
    output.world_position = vec4f(world, input.tint.z);
    output.normal_sky = vec4f(normal, sky);
    output.uv = input.uv; output.material = input.material;
    output.recipe = input.recipe.xyz;
    output.iris = input.iris; output.surface = surface;
    output.texture = input.surface >> 16u;
    output.hair_color_body = input.hair_color_body; output.cosmetics = input.cosmetics.xyz;
    return output;
}
@vertex fn vs_main(input: Input) -> Output {
    return character_vertex(input, false);
}
@vertex fn vs_shadow(input: Input) -> Output {
    return character_vertex(input, true);
}
fn srgb_to_linear(rgb: vec3f) -> vec3f {
    return select(pow((rgb+vec3f(0.055))/1.055,vec3f(2.4)),rgb/12.92,rgb<=vec3f(0.04045));
}
// The texture is already decoded by its sRGB texture format. Decode the user
// RGB exactly once, multiply in linear light. Accessory primitives never tint.
fn tint_hair(neutral: vec3f, rgb: vec3f) -> vec3f { return neutral * srgb_to_linear(rgb / 255.0); }
fn shade_hair(neutral: vec4f, rgb: vec3f, fixed: bool) -> vec4f {
    return vec4f(select(tint_hair(neutral.rgb,rgb),neutral.rgb,fixed),neutral.a);
}
fn character_albedo(input: Output) -> vec4f {
    if input.radiance_eye_height.w > 0.0 && (input.joint == 3u || input.joint == 4u) && input.direction_height.w > input.radiance_eye_height.w - 0.12 { discard; }
    let is_hair = input.material > 0u && input.material < 14u;
    if is_hair && input.material != input.recipe.z { discard; }
    var albedo = textureSampleLevel(body,pixels,input.uv,0,0.0);
    if is_hair {
        let fixed = select(0u,1u,input.surface == 8u);
        albedo = textureSampleLevel(hair,pixels,input.uv,i32(input.texture)-1,0.0);
        albedo = shade_hair(albedo,vec3f(input.hair_color_body.xyz),fixed != 0u);
    } else {
        if input.surface == 0u && input.cosmetics.x != 0u {
            // Preserve the painted shade ratios; palette entries remain the
            // same startup-validated colors used by the rest of the game.
            let shade = clamp(dot(albedo.rgb, vec3f(0.2126,0.7152,0.0722))/0.425405,0.0,1.0);
            albedo = vec4f(shade * SKINS[min(input.cosmetics.x,31u)],albedo.a);
        }
        if input.surface == 1u && input.cosmetics.z != 0u { albedo = vec4f(clamp(dot(albedo.rgb,vec3f(0.2126,0.7152,0.0722))/0.08,0.0,1.0) * PANTS[min(input.cosmetics.z,31u)],albedo.a); }
        if input.surface == 9u && input.cosmetics.y != 0u { albedo = vec4f(clamp(dot(albedo.rgb,vec3f(0.2126,0.7152,0.0722))/0.08,0.0,1.0) * SHIRTS[min(input.cosmetics.y,31u)],albedo.a); }
        if input.surface == 4u && input.iris.w != 0u { albedo = vec4f(srgb_to_linear(vec3f(input.iris.xyz)/255.0),albedo.a); }

    }
    if albedo.a < 0.05 { discard; }
    return albedo;
}
@fragment fn fs_main(input: Output) -> BgSceneOutput {
    let receiver = bg_shadow_receiver(input.world_position.xyz);
    let albedo = character_albedo(input);
    // Evaluate local shadows after interpolation; indirect/bounce stays intact.
    let local_light = bg_shadowed_local_light(input.world_position.xyz, normalize(input.normal_sky.xyz), input.radiance_eye_height.xyz, input.direction_height.xyz);
    let tint = vec3f(input.light.w, input.indirect.w, input.world_position.w);
    let light = input.light.xyz + tint * local_light - input.direct.xyz * (1.0 - bg_sun_visibility(receiver));
    // Mark local-source influence reactive before a moving shadow reaches it.
    let history_sign = bg_local_history_sign(input.world_position.xyz, normalize(input.normal_sky.xyz), input.radiance_eye_height.xyz, input.direction_height.xyz);
    return bg_scene_output(albedo.rgb*light,albedo.rgb*input.indirect.xyz,input.world_position.xyz,input.normal_sky.w,input.direct.w * history_sign);
}
@fragment fn fs_shadow(input: Output) {
    // Keep body, chosen hair and embedded texture alpha identical to color.
    // eye_height is zero in this pass, so first-person clipping never cuts it.
    _ = character_albedo(input);
}

// Reuse the exact color geometry and discard rules; only unused lighting outputs
// carry the interpolated previous clip position in this auxiliary pass.
@vertex fn vs_motion(input: Input) -> Output {
    var out = character_vertex(input, false);
    let previous = motion_frame.previous * previous_world[input.instance * 30u + input.joint] * vec4f(input.local, 1.0);
    out.light = vec4f(previous.xyz, out.light.w); out.normal_sky.w = previous.w; out.direct = out.clip;
    return out;
}
@fragment fn fs_motion(input: Output) -> @location(0) vec4f {
    _ = character_albedo(input);
    return bg_encode_motion(vec4f(input.light.xyz, input.normal_sky.w), input.direct);
}
