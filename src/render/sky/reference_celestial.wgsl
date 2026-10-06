// gbuffers_skytextured.glsl:69,78-93,106-109; default settings and
// lightColor.glsl NIGHT_MOON_PHASE. Input must be filtered encoded RGB,
// before hardware sRGB conversion. Output is before ALPHA_BLEND's sqrt:
// this renderer's existing sky attachment stores linear HDR radiance.
fn bg_bsl_textured_celestial(encoded:vec4f,up:f32,moon:bool,sun_visibility:f32,moon_multiplier:f32)->vec3f {
    var radiance=pow(max(encoded.rgb,vec3f(0.0)),vec3f(2.2))*encoded.a;
    let horizon=1.0-pow(1.0-max(up*0.975+0.025,0.0),8.0);
    let fade=smoothstep(0.0,1.0,horizon);
    radiance*=2.25*fade*fade;
    if moon {
        let night_palette=vec3f(96.0,192.0,255.0)*(0.3*moon_multiplier/255.0);
        let desaturated=dot(radiance,vec3f(0.299,0.587,0.114))*pow(night_palette,vec3f(1.6))*4.0;
        radiance=mix(desaturated,radiance,sun_visibility);
    }
    return radiance;
}
// DrawStars modern Overworld/Iris fallback, engine bedrockLevel=-64.
// Apply at the reference consumer only; enhanced star visibility is unchanged.
fn bg_bsl_reference_star_fade(eye_y:f32,reference:bool)->f32 {
    return select(1.0,clamp((eye_y+70.0)/8.0,0.0,1.0),reference);
}
