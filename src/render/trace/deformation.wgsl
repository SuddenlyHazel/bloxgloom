struct DeformFrame {inverse:mat4x4f,previous:mat4x4f,eye:vec4f};
struct DeformTriangle {a:vec4f,b:vec4f,c:vec4f,uv_ab:vec4f,uv_c:vec4f,normal:vec4f};
struct DeformMaterial {flags:u32,layer:u32};
@group(0) @binding(0) var<uniform> deform_frame:DeformFrame;
@group(0) @binding(1) var<storage,read> deform_source:array<DeformTriangle>;
@group(0) @binding(2) var<storage,read_write> deform_output:array<DeformTriangle>;
@group(1) @binding(5) var<storage,read> deform_materials:array<DeformMaterial>;

fn deform_position(p:vec3f,n:vec3f,uv:vec2f,flags:u32)->vec3f {
    if (flags&64u)==0u {return p;}
    var wind_uv=uv;
    if (flags&8u)!=0u {wind_uv.y=select(0.5+0.5*uv.y,0.5*uv.y,(flags&128u)!=0u);}
    return bg_foliage_wind(p,n,wind_uv,deform_frame.eye.w);
}
@compute @workgroup_size(64) fn deform(@builtin(global_invocation_id) invocation:vec3u) {
    let index=invocation.x;
    if index>=arrayLength(&deform_source) {return;}
    var t=deform_source[index];
    let flags=deform_materials[u32(t.a.w)].flags;
    t.a=vec4f(deform_position(t.a.xyz,t.normal.xyz,t.uv_ab.xy,flags),t.a.w);
    t.b=vec4f(deform_position(t.b.xyz,t.normal.xyz,t.uv_ab.zw,flags),t.b.w);
    t.c=vec4f(deform_position(t.c.xyz,t.normal.xyz,t.uv_c.xy,flags),t.c.w);
    deform_output[index]=t;
}
