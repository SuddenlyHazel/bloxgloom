// True current static samples, captured before the primary temporal mix.
@group(0) @binding(0) var raw:texture_2d_array<f32>;
@group(0) @binding(1) var geometry:texture_2d<f32>;
@group(0) @binding(2) var old_moments:texture_2d<f32>;
@group(0) @binding(3) var old_guide:texture_2d<f32>;
struct Output { @location(0) moments:vec4f,@location(1) guide:vec4f };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return vec4f(xy*2.0-1.0,0.0,1.0);
}
fn luminance(rgb:vec3f)->f32 {return dot(rgb,vec3f(0.2126,0.7152,0.0722));}
@fragment fn fs_moments(@builtin(position) frag:vec4f)->Output {
    let p=vec2i(frag.xy);let reflection=textureLoad(raw,p,1,0);
    if reflection.w<0.0 {return Output(vec4f(0.0),vec4f(0.0));}
    let total=textureLoad(raw,p,0,0).rgb;let metadata=textureLoad(raw,p,2,0);
    let r=luminance(reflection.rgb);let t=luminance(total-reflection.rgb);
    var moments=vec4f(r,r*r,t,t*t);var age=1.0;
    let hp=vec2i(metadata.zw);let size=vec2i(textureDimensions(old_moments));
    // hp is the exact pixel selected and accepted in primary FP32 reprojection,
    // not a second projection from the half-float output depth.
    if all(hp>=vec2i(0))&&all(hp<size) {
        let previous_guide=textureLoad(old_guide,hp,0);
        if previous_guide.w>0.0 {
            // An unsupported previous interface never contributes moments. Its
            // first supported successor restarts this independent confidence
            // sequence; the original primary static mean/age remain unchanged.
            age=min(textureLoad(geometry,p,0).w,previous_guide.w+1.0);
            moments=mix(moments,textureLoad(old_moments,hp,0),1.0-1.0/age);
        }
    }
    return Output(moments,vec4f(metadata.xy,reflection.w,age));
}
