// Opt-in only: absent from the default production shader and binding layout.
@group(0) @binding(16) var ray_lobe_raw:texture_storage_2d_array<rgba32float,write>;
fn ray_store_water_lobes(pixel:vec2i,total:vec3f,reflection:vec3f,
    primary_t:f32,normal:vec3f,roughness:f32,eligible:bool,previous_pixel:vec2i) {
    textureStore(ray_lobe_raw,pixel,0,vec4f(total,primary_t));
    let admitted=eligible&&all(textureDimensions(ray_lobe_raw)==textureDimensions(ray_history));
    textureStore(ray_lobe_raw,pixel,1,vec4f(reflection,select(-1.0,roughness,admitted)));
    textureStore(ray_lobe_raw,pixel,2,vec4f(oct_encode(normal),vec2f(previous_pixel)));
}
