struct BgSceneOutput { @location(0) color: vec4f, @location(1) indirect: vec4f };
fn bg_scene_output(color: vec3f, indirect: vec3f, world: vec3f, sky: f32, visibility: f32) -> BgSceneOutput {
    return BgSceneOutput(vec4f(bg_apply_fog(color, world, sky), 1.0),
        vec4f(max(indirect,vec3f(0.0))*bg_fog_transmittance(world,sky),visibility));
}
