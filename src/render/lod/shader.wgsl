struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
struct Tile { relative:vec4f, origin:vec4i };
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<storage,read> coverage:array<vec4i>;
@group(1) @binding(0) var<uniform> tile:Tile;
struct In { @location(0) position:vec3f,@location(1) normal:vec3f,@location(2) color:vec3f,@location(3) light:vec2f };
struct Out { @builtin(position) position:vec4f,@location(0) relative:vec3f,@location(1) local:vec3f,@location(2) color:vec3f,@location(3) sky:f32,@location(4) normal:vec3f,@location(5) indirect:vec3f };
@vertex fn vs_main(v:In)->Out {
    var o:Out;o.relative=v.position+tile.relative.xyz;o.position=camera.view_projection*vec4f(o.relative,1.0);o.local=v.position;o.normal=v.normal;o.sky=v.light.x;
    // Use the calibrated near-surface basis. Distant geometry does not sample
    // the camera-local shadow map, whose range is inside the near chunk band.
    // Far span snapshots currently retain scalar glow only; preserve their
    // existing warm approximation until their transport schema carries RGB.
    let light=bg_surface_light(v.normal,camera.sun,v.light.x,v.light.y*v.light.y*vec3f(1.0,0.57,0.23),vec3f(0.0),vec3f(0.0),1.0);
    o.color=v.color*light;o.indirect=v.color*bg_indirect_daylight(v.normal,camera.sun,v.light.x);return o;
}
@fragment fn fs_main(v:Out)->BgSceneOutput {
    // Sample just inside the span, so a boundary face belongs to its source
    // chunk. Coverage is full 3D and includes ready known-empty near chunks.
    let world=vec3i(floor(v.local-v.normal*0.002))+tile.origin.xyz;
    let k=world >> vec3u(4u);
    var index=(u32(k.x)*73856093u ^ u32(k.y)*19349663u ^ u32(k.z)*83492791u)&16383u;
    for(var probe=0u;probe<16384u;probe++){
        let c=coverage[index];if c.w==0 {break;}if all(c.xyz==k){discard;}index=(index+1u)&16383u;
    }
    // Outdoor air scatters even an unlit distant wall toward the horizon.
    // A sheltered viewer still sees sealed, unlit interiors remain dark.
    return bg_scene_output(v.color,v.indirect,v.relative,max(v.sky,camera.eye.w),1.0);
}
