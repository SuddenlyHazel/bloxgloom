// No storage global, layout entry, resource, or write in the off control.
@fragment fn fs_main(@builtin(position) frag:vec4f)->Output {
    return attachments(vec2u(frag.xy));
}
