@group(0) @binding(1) var raw:texture_storage_2d_array<rgba32float,write>;
@fragment fn fs_main(@builtin(position) frag:vec4f)->Output {
    let p=vec2u(frag.xy);
    textureStore(raw,vec2i(p),0,raw_total(p));
    textureStore(raw,vec2i(p),1,raw_reflection(p));
    textureStore(raw,vec2i(p),2,exact_guide(p));
    return attachments(p);
}
