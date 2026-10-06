// Pickups retain continuous roughness/emission filtering, while labPBR metal
// IDs and porosity/subsurface selectors come from their exact base texel.
fn dyn_catalog_channels(uv:vec2f,layer:i32,lab:bool)->vec4f {
    let filtered=textureSampleLevel(ray_specular,ray_sampler,uv,layer,0.0);
    if !lab {return filtered;}
    let size=textureDimensions(ray_specular,0);
    let pixel=min(vec2i(floor(fract(uv)*vec2f(size))),vec2i(size)-vec2i(1));
    let categorical=textureLoad(ray_specular,pixel,layer,0);
    return vec4f(filtered.r,categorical.g,categorical.b,filtered.a);
}
