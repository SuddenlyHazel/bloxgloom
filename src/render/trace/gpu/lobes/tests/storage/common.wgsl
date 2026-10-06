// Frozen independent storage gate. Binary-exact arithmetic keeps this a data
// preservation oracle, not a compiler comparison of the transport integrator.
struct Frame { index:vec4u };
@group(0) @binding(0) var<uniform> frame:Frame;
struct Output {
    @location(0) a:vec4f,@location(1) b:vec4f,
    @location(2) c:vec4f,@location(3) d:vec4f,
};
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));
    return vec4f(xy*2.0-1.0,0.0,1.0);
}
fn raw_total(p:vec2u)->vec4f {
    return vec4f(select(14721.34,-0.0009765625,(p.x&1u)!=0u),
        f32(p.y)*0.125,-f32(p.x+1u)*0.0625,select(1.0,-0.75,frame.index.x!=0u));
}
fn raw_reflection(p:vec2u)->vec4f {
    let invalid=p.x==7u||(frame.index.x!=0u&&(p.y&1u)!=0u);
    return vec4f(f32(p.x+p.y*8u+frame.index.x*64u)*0.5,
        0.00000095367431640625,-16384.0,select(0.125,-1.0,invalid));
}
fn exact_guide(p:vec2u)->vec4f {
    if raw_reflection(p).w<0.0 {return vec4f(0.0,0.0,-1.0,-1.0);}
    return vec4f(f32(p.x)*0.125-0.5,f32(p.y)*0.125-0.5,
        select(0.0,16383.0,(p.x&1u)!=0u),f32(p.y+frame.index.x*8u));
}
fn attachments(p:vec2u)->Output {
    return Output(vec4f(f32(p.x)*0.125,f32(p.y)*0.125,f32(frame.index.x),1.0),
        raw_total(p),raw_reflection(p),exact_guide(p));
}
