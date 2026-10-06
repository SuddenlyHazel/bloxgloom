@group(0) @binding(0) var raw:texture_2d_array<f32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));
    return vec4f(xy*2.0-1.0,0.0,1.0);
}
// A 24x8 output stores the three 8x8 array layers side by side.
@fragment fn fs_main(@builtin(position) frag:vec4f)->@location(0) vec4f {
    let p=vec2i(frag.xy);
    return textureLoad(raw,vec2i(p.x%8,p.y),p.x/8,0);
}
