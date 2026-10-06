// Independently expressed local comparison to supplied BSL CLOUDS=2 defaults.
// Reference: supplied BSL by Capt Tatsu, lib/atmospherics/clouds.glsl.
// The original user-provided noise remains an external runtime input.
@group(0) @binding(1) var bg_reference_noise:texture_2d<f32>;
@group(0) @binding(2) var bg_reference_noise_sampler:sampler;
fn bg_reference_cloud_bayer2(p:vec2f)->f32 {let q=floor(p);return fract(q.x*0.5+q.y*0.75);}
fn bg_reference_cloud_dither(p:vec2f,frame:f32)->f32 {
    let four=bg_reference_cloud_bayer2(p*0.25)*0.25+bg_reference_cloud_bayer2(p*0.5);
    return fract(four*0.25+bg_reference_cloud_bayer2(p)+frame*0.618);
}
fn bg_reference_cloud_density(base:f32,detail:f32,gradient:f32,reveal:f32,rain:f32)->f32 {
    let edge=abs(gradient-0.125)*select(8.0,1.14,gradient>0.125);
    let coverage=edge*edge*4.0;
    var density=mix(base,detail,0.0476)*21.0;
    density=mix(density-coverage,21.0-coverage*2.5,rain*0.33);
    density=max(density-(reveal*3.0+10.0),0.0)*0.5*(1.0-rain*0.75);
    return density*inverseSqrt(density*density+0.5);
}
fn bg_reference_cloud_sample(position:vec3f,wind:vec2f,reveal:f32,rain:f32)->f32 {
    let gradient=clamp((position.y-192.0)/60.0,0.0,1.0);
    let coord=position.xz*(0.004/12.0);
    let base=textureSampleLevel(bg_reference_noise,bg_reference_noise_sampler,coord*0.25+wind,0.0).r;
    let slices=gradient*5.0;
    let detail_coord=coord*0.5-wind*2.0+floor(slices)*0.04;
    let low=textureSampleLevel(bg_reference_noise,bg_reference_noise_sampler,detail_coord,0.0).b;
    let high=textureSampleLevel(bg_reference_noise,bg_reference_noise_sampler,detail_coord+0.04,0.0).b;
    return bg_reference_cloud_density(base,mix(low,high,fract(slices)),gradient,reveal,rain);
}
// Source sky-depth case z=1, including water's fadeFaster reflection branch.
// Reflection origin is the absolute surface position; source step scaling and
// underground attenuation still use the actual camera altitude.
fn bg_reference_cloud_integrate_case(origin:vec3f,ray:vec3f,pixel:vec2f,camera:SkyCamera,eye_height:f32,fade_faster:bool)->vec4f {
    if abs(ray.y)<0.000001 {return vec4f(0.0,0.0,0.0,1.0);}
    let lower=(192.0-origin.y)/ray.y;let upper=(252.0-origin.y)/ray.y;
    let nearest=max(min(lower,upper),0.0);let furthest=max(lower,upper);
    if furthest<0.0 {return vec4f(0.0,0.0,0.0,1.0);}
    let scaling=clamp((abs(eye_height-222.0)/30.0-1.0)*0.625,0.0,1.0);
    let step_length=30.0/(4.0*ray.y*ray.y*scaling+1.0);
    let count=u32(min((furthest-nearest)/step_length,32.0)+1.0);
    let dither=bg_reference_cloud_dither(pixel,camera.reference.z);
    let time=camera.climate.w;let rain=camera.climate.x;
    let wind=vec2f(time*0.0005,sin(time*0.001)*0.005)*0.667;
    let sun=normalize(camera.sun.xyz);
    let light=sun*select(1.0,-1.0,camera.reference.w<0.0);
    let vl=dot(ray,light);let vs=dot(ray,sun);let shadow=camera.reference.x;
    let reveal=pow(clamp(mix(abs(vl),max(vl,0.0),shadow)*2.0-1.0,0.0,1.0),12.0)*(1.0-rain);
    let half_light=mix(abs(vl)*0.8,vl,shadow)*0.5+0.5;
    let scattering=pow(half_light,6.0);let light_factor=(2.0-1.5*vl*shadow)*2.0;
    let fog=max(camera.reference.y,0.5);let fade_start=32.0/fog;let fade_end=select(240.0,80.0,fade_faster)/fog;
    var opacity=0.0;var lighting=0.0;var fade=1.0;
    for(var i=0u;i<count;i++) {
        if opacity>0.99 {break;}
        let position=origin+ray*(nearest+step_length*(f32(i)+dither));
        let gradient=clamp((position.y-192.0)/60.0,0.0,1.0);
        var density=bg_reference_cloud_sample(position,wind,reveal,rain);
        density*=select(0.0,1.0,position.y>=192.0&&position.y<=252.0);
        let distance=length(position.xz-origin.xz)*(10.0/73.0);
        let sample_light=(pow(gradient,1.125*half_light*half_light+0.875)*0.8+0.2)*(1.0-pow(density,light_factor));
        let sample_fade=clamp((distance-fade_end)/(fade_start-fade_end),0.0,1.0);
        fade*=mix(1.0,sample_fade,density*(1.0-opacity));
        density*=select(0.0,1.0,distance<=fade_end);
        lighting=mix(lighting,sample_light,density*(1.0-opacity*opacity));
        opacity=mix(opacity,1.0,density);
    }
    lighting=mix(lighting,1.0,(1.0-opacity*opacity)*scattering*0.5)*(1.0-0.9*rain);
    let visible=clamp(sun.y*10.0+0.5,0.0,1.0);let sky_visible=clamp(sun.y*2.0+0.5,0.0,1.0);
    let horizon_mix=sky_visible*(1.0-visible)*pow(vs*0.5+0.5,2.0)*0.5;
    let horizon=pow(bg_bsl_daylight_palette(camera.sun_radiance.w),vec3f(4.0-3.0*sky_visible));
    let ambient=bg_bsl_ambient_default(sun,camera.sun_radiance.w,rain,camera.climate.y)*(0.3*sky_visible+0.5);
    let direct=mix(camera.sun_radiance.xyz,horizon,horizon_mix)*(0.85+1.15*scattering);
    var color=mix(ambient,direct,lighting)*(1.0-0.4*rain)*(0.5-0.25*(1.0-sky_visible)*(1.0-rain));
    color*=clamp((eye_height+70.0)/8.0,0.0,1.0);
    opacity*=fade;opacity*=opacity;
    return vec4f(color*opacity,1.0-opacity);
}
fn bg_reference_cloud_integrate(origin:vec3f,ray:vec3f,pixel:vec2f,camera:SkyCamera)->vec4f {
    return bg_reference_cloud_integrate_case(origin,ray,pixel,camera,origin.y,false);
}
