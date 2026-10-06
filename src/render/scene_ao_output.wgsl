struct BgSceneOutput { @location(0) color: vec4f, @location(1) indirect: vec4f,
    @location(2) reflection_normal:vec4f,@location(3) reflection_response:vec4f };
fn bg_scene_output(color: vec3f, indirect: vec3f, world: vec3f, sky: f32, visibility: f32) -> BgSceneOutput {
    return BgSceneOutput(vec4f(bg_apply_fog(color, world, sky), 1.0),
        vec4f(max(indirect,vec3f(0.0))*bg_fog_transmittance(world,sky),visibility),vec4f(0.0),vec4f(0.0));
}
fn bg_scene_reflection(output:BgSceneOutput,normal:vec3f,roughness:f32,linear_distance:f32,weight:vec3f,sky_visibility:f32)->BgSceneOutput {
    var result=output;
    let unit=normalize(normal);
    let n=unit/(abs(unit.x)+abs(unit.y)+abs(unit.z));
    var encoded=n.xy;
    if n.z<0.0 {encoded=(vec2f(1.0)-abs(n.yx))*select(vec2f(-1.0),vec2f(1.0),n.xy>=vec2f(0.0));}
    result.reflection_normal=vec4f(encoded,roughness,linear_distance);
    result.reflection_response=vec4f(weight,clamp(sky_visibility,0.0,1.0));
    return result;
}
