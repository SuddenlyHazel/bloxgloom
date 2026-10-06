@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var noise: texture_2d<f32>;
@group(0) @binding(2) var linear_sampler: sampler;
@group(0) @binding(3) var noise_sampler: sampler;
// temporal enabled, effects enabled, output sRGB attachment, local noise available
@group(0) @binding(4) var<uniform> options: vec4f;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[id],0.0,1.0);
}
fn bg_encode_srgb(color:vec3f)->vec3f {return select(1.055*pow(max(color,vec3f(0.0)),vec3f(1.0/2.4))-0.055,color*12.92,color<=vec3f(0.0031308));}
fn bg_decode_srgb(color:vec3f)->vec3f {return select(pow((color+0.055)/1.055,vec3f(2.4)),color/12.92,color<=vec3f(0.04045));}
fn sample_color(uv:vec2f)->vec3f {return textureSampleLevel(source,linear_sampler,uv,0.0).rgb;}
fn luma(color:vec3f)->f32 {return dot(color,vec3f(0.299,0.587,0.114));}
@fragment fn gamma_grain(@builtin(position) p:vec4f)->@location(0) vec4f {
    let color=max(textureLoad(source,vec2i(p.xy),0).rgb,vec3f(0.0));
    if options.y<0.5 {return vec4f(bg_encode_srgb(color),1.0);}
    var encoded=pow(color,vec3f(1.0/2.2));
    if options.w>0.5 {
        let height=f32(textureDimensions(source).y);
        let gl_pixel=vec2f(p.x,height-p.y);
        encoded+=(textureSampleLevel(noise,noise_sampler,gl_pixel/512.0,0.0).b-0.5)/256.0;
    }
    return vec4f(encoded,1.0);
}
@fragment fn present(@builtin(position) p:vec4f)->@location(0) vec4f {
    var encoded=textureLoad(source,vec2i(p.xy),0).rgb;
    if options.z>0.5 {encoded=bg_decode_srgb(encoded);}
    return vec4f(encoded,1.0);
}
@fragment fn antialias(@builtin(position) p:vec4f)->@location(0) vec4f {
    let uv=p.xy/vec2f(textureDimensions(source));let color=sample_color(uv);
    if options.y<0.5 {return vec4f(color,1.0);}
    return vec4f(bg_reference_fxaa(color,vec2f(uv.x,1.0-uv.y)),1.0);
}
