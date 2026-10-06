// Exact DYNAMIC_HANDLIGHT=2 defaults. Inputs are source raw lightmap values,
// authoritative selected-stack block emission, and camera-relative position.
fn bg_bsl_handlight(lightmap:vec2f,relative:vec3f,relative_eye:vec3f,held:f32)->vec2f {
    if held==0.0 {return lightmap;}
    let position=relative+relative_eye+vec3f(0.0,0.5,0.0);
    let hand=min((held-2.0*length(position))/15.0,0.9333);
    let block=log2(exp2(lightmap.x*32.0)+exp2(hand*32.0))/32.0;
    return vec2f(block,lightmap.y);
}
fn bg_bsl_handlight_hand(lightmap:vec2f,held:f32)->vec2f {
    return vec2f(max(lightmap.x,min(held/15.0,0.9333)),lightmap.y);
}
