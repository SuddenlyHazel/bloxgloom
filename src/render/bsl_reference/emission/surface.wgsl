// terrain.glsl computes GetHardcodedEmission on encoded, tinted RGB BEFORE
// pow(albedo,2.2). Our reference albedo has already made that conversion.
fn bg_bsl_hardcoded_emission(encoded:vec3f)->f32 {
    let value=max(max(encoded.r,encoded.g),encoded.b);
    let minimum=min(min(encoded.r,encoded.g),encoded.b);
    // Only HSV saturation/value are consumed; source RGB2HSV's hue is unused.
    let saturation=(value-minimum)/(value+1.0e-10);
    let saturated=clamp(saturation*3.125-0.125,0.0,1.0)*pow(value,4.0);
    let desaturated=clamp(value*7.0-6.0,0.0,1.0);
    return max(saturated,desaturated)*0.5;
}
fn bg_bsl_default_emission(layer:u32,source_albedo:vec3f)->f32 {
    let kind=bg_bsl_emission_class(layer);
    if kind<150u||kind>158u {return 0.0;}
    return bg_bsl_hardcoded_emission(pow(max(source_albedo,vec3f(0.0)),vec3f(1.0/2.2)));
}
