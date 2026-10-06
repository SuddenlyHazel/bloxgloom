// BSL lib/vertex/waving.glsl: default strength/speed1, all botanical waving
// options enabled, WORLD_TIME_ANIMATION disabled. No enhanced sine fallback.
fn bg_bsl_wind_noise(position:vec2f)->f32 {
    return fract(sin(dot(position,vec2f(12.9898,4.1414)))*43758.5453);
}
fn bg_bsl_wind_noise_2d(position:vec2f)->f32 {
    let cell=floor(position);let fraction=fract(position);
    let interpolant=fraction*fraction*(vec2f(3.0)-2.0*fraction);
    let n00=bg_bsl_wind_noise(cell);
    let n01=bg_bsl_wind_noise(cell+vec2f(0.0,1.0));
    let n10=bg_bsl_wind_noise(cell+vec2f(1.0,0.0));
    let n11=bg_bsl_wind_noise(cell+vec2f(1.0,1.0));
    return mix(mix(n00,n01,interpolant.y),mix(n10,n11,interpolant.y),interpolant.x)-0.5;
}
fn bg_bsl_wind_move(position:vec3f,seconds:f32,density:f32,speed:f32,multiplier:vec2f)->vec3f {
    let p=position*density+vec3f(seconds*speed);
    let wave=vec3f(bg_bsl_wind_noise_2d(p.yz),
        bg_bsl_wind_noise_2d(p.xz+vec2f(0.333)),bg_bsl_wind_noise_2d(p.xy+vec2f(0.667)));
    return wave*vec3f(multiplier,multiplier.x);
}
fn bg_bsl_grass_bend(relative:vec3f,relative_eye:vec3f)->vec3f {
    // The reference payload carries the same relativeEyePosition used by
    // source hand lighting; retain the checked-in source addition verbatim.
    let position=relative+relative_eye+vec3f(0.0,0.62,0.0);
    let modified=position*vec3f(4.0,2.0,4.0);
    let bend=vec3f(1.0,0.25,1.0)*max(2.0/max(length(modified),1.0)-0.35,0.0);
    return position*bend;
}
fn bg_bsl_foliage_wind(position:vec3f,uv:vec2f,seconds:f32,eye:vec3f,relative_eye:vec3f,layer:u32)->vec3f {
    let kind=bg_bsl_wind_class(layer);let top=uv.y<0.5;
    var wave=vec3f(0.0);
    if kind==100u&&top {
        wave=bg_bsl_wind_move(position,seconds,0.35,1.0,vec2f(0.25,0.06));
        wave+=bg_bsl_grass_bend(position-eye,relative_eye);
    }
    if kind==104u&&(top||fract(position.y+0.0675)>0.01) {
        wave=bg_bsl_wind_move(position,seconds,0.35,1.0,vec2f(0.15,0.06));
        wave+=bg_bsl_grass_bend(position-eye,relative_eye);
    }
    if kind==101u&&(top||fract(position.y+0.005)>0.01) {
        wave=bg_bsl_wind_move(position,seconds,0.7,1.35,vec2f(0.12,0.0));
        wave+=bg_bsl_grass_bend(position-eye,relative_eye);
    }
    if kind==107u {wave=bg_bsl_wind_move(position,seconds,0.5,1.25,vec2f(0.06,0.0));}
    if kind==108u {
        wave.y=(sin(2.0*3.1415927*(seconds*0.7+position.x*0.14+position.z*0.07))
            +sin(2.0*3.1415927*(seconds*0.5+position.x*0.10+position.z*0.20)))*0.0125;
    }
    if (kind==102u&&(top||fract(position.y+0.005)>0.01))||kind==103u {
        wave=bg_bsl_wind_move(position,seconds,0.35,1.15,vec2f(0.15,0.06));
    }
    if kind==105u {wave=bg_bsl_wind_move(position,seconds,0.25,1.0,vec2f(0.08,0.08));}
    if kind==106u {wave=bg_bsl_wind_move(position,seconds,0.35,1.25,vec2f(0.06,0.06));}
    return position+wave;
}
