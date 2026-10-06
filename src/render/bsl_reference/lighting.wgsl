// Pure checked-in default forwardLighting.glsl equations. Engine-specific
// bindings/input mappings are outside these functions and documented separately.
struct BgBslReferenceFrame {
    light:vec3f,ambient:vec3f,direction:vec3f,
    sun_visibility:f32,rain:f32,shadow_fade:f32,moon_multiplier:f32,
};
fn bg_bsl_vanilla_diffuse(normal:vec3f)->f32 {
    let up=clamp(normal.y,-1.0,1.0);let east=clamp(normal.x,-1.0,1.0);
    let value=(0.25*up+0.75)+(0.667-abs(east))*(1.0-abs(up))*0.15;
    return value*value;
}
fn bg_bsl_default_scene_light(normal:vec3f,view:vec3f,sky:f32,basic_subsurface:f32,
    shadow:f32,frame:BgBslReferenceFrame)->vec4f {
    var cosine=clamp(dot(normal,frame.direction)*1.01-0.01,0.0,1.0);
    var scattering=0.0;
    if basic_subsurface>0.0 {
        let vl=clamp(dot(-view,frame.direction)*0.5+0.5,0.0,1.0);
        scattering=pow(vl,16.0)*(1.0-frame.rain)*basic_subsurface*frame.shadow_fade;
        cosine=mix(cosine,1.0,sqrt(basic_subsurface)*0.7);
        cosine=mix(cosine,1.0,scattering);
    }
    let full_shadow=max(shadow*cosine,0.0);
    let shadow_mult=(1.0-0.95*frame.rain)*frame.shadow_fade;
    let scene=mix(frame.ambient*sky,frame.light,full_shadow*shadow_mult)
        *sky*sky*(1.0+scattering*shadow);
    return vec4f(scene,full_shadow);
}
fn bg_bsl_default_surface(albedo:vec3f,normal:vec3f,view:vec3f,lightmap:vec2f,
    smooth_lighting:f32,basic_subsurface:f32,emission_input:f32,shadow:f32,
    frame:BgBslReferenceFrame)->vec3f {
    let lm=clamp(lightmap,vec2f(0.0),vec2f(0.9333,1.0));
    let scene=bg_bsl_default_scene_light(normal,view,lm.y,basic_subsurface,shadow,frame);
    let block_raw=vec3f(255.0,212.0,160.0)*(0.85/255.0);
    let new_lightmap=pow(lm.x,10.0)*1.6+lm.x*0.6;
    let block=block_raw*block_raw*new_lightmap*new_lightmap;
    let minimum=pow(128.0*0.5/255.0,2.0)*0.04*(1.0-lm.y*lm.y);
    let emission=clamp(emission_input,0.0,1.0);
    let flatten=clamp(1.0-pow(1.0-emission,128.0),0.0,1.0);
    let emissive=mix(normalize(albedo+vec3f(0.00001)),vec3f(1.0),emission*0.5)*emission*4.0;
    let peak=max(max(albedo.x,albedo.y),albedo.z);
    let balanced=albedo/(1.0+peak*0.25*(1.0-flatten));
    let diffuse=mix(bg_bsl_vanilla_diffuse(normal),1.0,flatten);
    let smooth_factor=mix(smooth_lighting,1.0,flatten);
    var color=balanced*max(scene.rgb+block+emissive+vec3f(minimum),vec3f(0.0))*diffuse*smooth_factor*smooth_factor;
    // DESATURATION and DESATURATION_FACTOR=1.5 are enabled by default.
    var amount=1.0-sqrt(max(sqrt(length(vec3f(scene.a)/3.0))*lm.y,lm.y))
        *frame.sun_visibility*(1.0-frame.rain*0.7);
    amount*=smoothstep(0.25,1.0,(1.0-lm.x)*(1.0-lm.x))*(1.0-flatten);
    amount=1.0-amount;
    let night=vec3f(96.0,192.0,255.0)*(0.3*frame.moon_multiplier/255.0);
    let weather=vec3f(176.0,224.0,255.0)*(1.2/255.0);
    let hue=mix(normalize(weather*weather+vec3f(0.000001)),normalize(night*night+vec3f(0.000001)),
        (1.0-frame.sun_visibility)*(1.0-frame.rain));
    let tint=mix(vec3f(0.4),hue,sqrt(lm.y))*1.7;
    let gray=dot(color,vec3f(0.299,0.587,0.114));
    let desaturated=mix(color,gray*tint,0.4);
    return mix(desaturated,color,amount);
}
