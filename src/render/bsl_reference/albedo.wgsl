// BSL samples encoded texture RGB (including source vertex tint), then applies
// pow(RGB,2.2). The engine texture array already performs hardware sRGB EOTF.
// Recover the encoded value without changing imported pixels or enhanced mode.
fn bg_bsl_texture_albedo(linear:vec3f)->vec3f {
    let positive=max(linear,vec3f(0.0));
    let encoded=select(1.055*pow(positive,vec3f(1.0/2.4))-0.055,
        positive*12.92,positive<=vec3f(0.0031308));
    return pow(max(encoded,vec3f(0.0)),vec3f(2.2));
}
fn bg_reference_texture_albedo(sample:vec4f)->vec4f {
    if BG_BSL_REFERENCE||BG_BSL_ADVANCED_REFERENCE {
        return vec4f(bg_bsl_texture_albedo(sample.rgb),sample.a);
    }
    return sample;
}
