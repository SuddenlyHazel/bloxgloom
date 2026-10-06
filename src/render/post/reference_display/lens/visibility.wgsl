@group(0) @binding(0) var depth:texture_depth_2d;
@group(0) @binding(1) var previous:texture_2d<f32>;
@group(0) @binding(2) var<uniform> lens:BgReferenceLens;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position)vec4f {
    let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[id],0.0,1.0);
}
@fragment fn visibility()->@location(0)f32 {
    let size=vec2i(textureDimensions(depth));let uv=lens.optical.xy+0.5;
    let pixel=clamp(vec2i(vec2f(uv.x,1.0-uv.y)*vec2f(size)),vec2i(0),size-1);
    let visible=f32(textureLoad(depth,pixel,0)>=1.0)*(1.0-lens.celestial.z)*lens.night.w*lens.state.y;
    let old=textureLoad(previous,vec2i(0),0).r*lens.state.x;
    return mix(visible,old,exp2(-lens.celestial.w*12.5));
}
