struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
@group(0) @binding(0) var<uniform> camera:Camera;
@group(1) @binding(0) var<uniform> clock:vec4f;
struct Input { @location(0) position:vec3f,@location(1) normal:vec3f,@location(2) color:vec4f,@location(3) light:vec2f };
struct Output { @builtin(position) position:vec4f,@location(0) world:vec3f,@location(1) normal:vec3f,@location(2) color:vec4f,@location(3) light:vec2f };
@vertex fn vs(v:Input)->Output { return Output(camera.view_projection*vec4f(v.position,1.0),v.position,v.normal,v.color,v.light); }
@fragment fn fs(v:Output,@builtin(front_facing) front:bool)->BgSceneOutput {
    let receiver = bg_shadow_receiver(v.world);
    return bg_water_surface(v.color, v.normal, v.light.x, v.light.y, v.world, v.world-camera.eye.xyz, front, clock.x, bg_sun_visibility(receiver));
}
