// Equations and default constants audited against the provided BSL source:
// external/shaders/program/composite5.glsl::BSLTonemap and
// external/shaders/lib/atmospherics/sky.glsl::GetSkyColor.
// The default curves are 1/1/2 and WHITE_PATH=1, exposure stop offset=2.
fn bg_bsl_tonemap_default(linear:vec3f)->vec3f {
    let desaturate=mat3x3f(vec3f(0.96,0.03,0.01),vec3f(0.03,0.94,0.03),vec3f(0.01,0.03,0.96));
    let restore=mat3x3f(vec3f(1.04279930596,-0.03296703,-0.00983227299),vec3f(-0.03296703,1.06593406,-0.03296703),vec3f(-0.00983227299,-0.03296703,1.04279930596));
    let value=desaturate*max(linear,vec3f(0.0))*4.0;
    return clamp(restore*(value/sqrt(value*value+vec3f(1.0))),vec3f(0.0),vec3f(1.0));
}
fn bg_bsl_luminance(value:vec3f)->f32 {return dot(value,vec3f(0.299,0.587,0.114));}
fn bg_bsl_daylight_palette(time_brightness:f32)->vec3f {
    let fade=1.0-pow(1.0-clamp(time_brightness,0.0,1.0),1.5);
    return mix(vec3f(255.0,160.0,80.0)*1.2,vec3f(196.0,220.0,255.0)*1.4,fade)/255.0;
}
fn bg_bsl_ambient_palette(time_brightness:f32)->vec3f {
    let fade=1.0-pow(1.0-clamp(time_brightness,0.0,1.0),1.5);
    let day=mix(vec3f(255.0,204.0,144.0)*0.35,vec3f(120.0,172.0,255.0)*0.60,fade)/255.0;
    return day*day;
}
fn bg_bsl_ambient_default(sun:vec3f,time_brightness:f32,rain:f32,moon_multiplier:f32)->vec3f {
    let day=sqrt(bg_bsl_ambient_palette(time_brightness));
    let night=vec3f(96.0,192.0,255.0)*(0.60*0.3/255.0)*moon_multiplier;
    let raw=mix(night,day,clamp(sun.y*10.0+0.5,0.0,1.0));
    let weather_color=vec3f(176.0,224.0,255.0)*(1.2/255.0);
    let tinted=mix(raw,bg_bsl_luminance(raw)*weather_color,clamp(rain,0.0,1.0));
    return tinted*tinted;
}
// Custom sky defaults: SKY_VANILLA and USE_FOG_COLOR are disabled in the
// supplied settings. Moon strength follows the server day-index
// cycle. Weather biome weights and edited shader profiles remain distinct inputs.
fn bg_bsl_sky_default(direction:vec3f,sun:vec3f,time_brightness:f32,rain:f32,moon_multiplier:f32)->vec3f {
    let up=clamp(direction.y,-1.0,1.0);let toward=clamp(dot(direction,sun),-1.0,1.0);
    let day=clamp(sun.y*2.0+0.5,0.0,1.0);
    let sun_visible=clamp(sun.y*10.0+0.5,0.0,1.0);
    let brightness=clamp(time_brightness,0.0,1.0);let weather=clamp(rain,0.0,1.0);
    let curve=mix(1.5,1.0,toward);
    let gradient=exp(-(1.0-pow(1.0-max(up,0.0),curve))/0.35);
    let base=vec3f(96.0,160.0,255.0)/255.0;
    var sky=base*base*gradient;
    sky=sky/sqrt(sky*sky+vec3f(1.0))*exp2(brightness*0.75-0.75)*day;
    let sun_mix=pow((toward*0.5+0.5)*clamp(1.0-up,0.0,1.0),2.0-day)*pow(1.0-brightness*0.6,3.0);
    let horizon_mix=pow(1.0-abs(up),2.5)*0.125;
    let light_mix=1.0-(1.0-sun_mix)*(1.0-horizon_mix);
    var light_sky=pow(bg_bsl_daylight_palette(brightness),vec3f(4.0-day))*gradient;
    light_sky=light_sky/(vec3f(1.0)+light_sky*weather);
    sky=mix(sqrt(max(sky*(1.0-light_mix),vec3f(0.0))),sqrt(light_sky),light_mix);
    sky*=sky;
    let moon=vec3f(96.0,192.0,255.0)*(0.3/255.0)*moon_multiplier;
    let night=moon*moon*exp(-max(up,0.0)/0.65)*exp2(-3.5);
    let visibility=max(sun_visible,day);sky=mix(night,sky,visibility*visibility);
    let weather_color=vec3f(176.0,224.0,255.0)*(1.2/255.0);
    var weather_sky=weather_color*weather_color;
    let ambient=bg_bsl_ambient_default(sun,brightness,weather,moon_multiplier);
    weather_sky*=bg_bsl_luminance(ambient/weather_sky)*(0.2*day+0.2);
    sky=mix(sky,weather_sky*exp(-max(up,0.0)/1.5),weather);
    let ground_up=clamp(-up*1.015-0.015,0.0,1.0);
    let ground_density=0.1*(4.0-3.0*day)*(10.0*weather*weather+1.0);
    let ground=1.0-exp(-ground_density/max(ground_up,0.000001));
    return max(sky*ground,vec3f(0.0));
}

// Checked-in atmospherics/fog.glsl::GetFogColor, before runtime eye/bedrock
// attenuation. Air radiance shares the sky's linear palettes, not the legacy
// engine ambient-horizon calibration. Its gradients intentionally differ from
// the sky gradient and do not invent a white distance boundary.
fn bg_bsl_fog_default(direction:vec3f,distance:f32,sun:vec3f,time_brightness:f32,rain:f32,moon_multiplier:f32)->vec3f {
    let up=clamp(direction.y,-1.0,1.0);let toward=clamp(dot(direction,sun),-1.0,1.0);
    let day=clamp(sun.y*2.0+0.5,0.0,1.0);
    let brightness=clamp(time_brightness,0.0,1.0);let weather=clamp(rain,0.0,1.0);
    let gradient=exp(-(up*0.5+0.5)*0.5/0.4);
    let base=vec3f(96.0,160.0,255.0)/255.0;
    var fog=base*base*gradient;
    fog=fog/sqrt(fog*fog+vec3f(1.0))*exp2(brightness*0.75-0.75)*day;
    let sun_mix=pow((toward*0.5+0.5)*clamp(1.0-up,0.0,1.0),2.0-day)*pow(1.0-brightness*0.6,3.0);
    let horizon_mix=pow(1.0-abs(up),2.5)*0.125;
    let view_mix=1.0-exp(-pow(distance/64.0,2.0));
    let light_mix=(1.0-(1.0-sun_mix)*(1.0-horizon_mix))*view_mix;
    var light_fog=pow(bg_bsl_daylight_palette(brightness),vec3f(4.0-day))*gradient;
    light_fog/=vec3f(1.0)+light_fog*weather;
    fog=mix(sqrt(max(fog*(1.0-light_mix),vec3f(0.0))),sqrt(light_fog),light_mix);
    fog*=fog;
    let night=vec3f(96.0,192.0,255.0)*(0.3*moon_multiplier/255.0);
    fog=mix(night*night*exp(-(up*0.5+0.5)*0.35)*exp2(-3.5),fog,day*day);
    let weather_color=vec3f(176.0,224.0,255.0)*(1.2/255.0);
    var weather_fog=weather_color*weather_color;
    let ambient=bg_bsl_ambient_default(sun,brightness,weather,moon_multiplier);
    weather_fog*=bg_bsl_luminance(ambient/weather_fog)*(0.2*day+0.2);
    return max(mix(fog,weather_fog*exp(-(up*0.5+0.5)*0.125/1.5),weather),vec3f(0.0));
}

// Default DrawStars from lib/atmospherics/clouds.glsl. World/view rotation
// gives direction*100; its scale cancels in the plane projection. The live
// presentation clock supplies frameTimeCounter, rather than day-phase wind.
fn bg_bsl_star_noise(p:vec2f)->f32 {return fract(sin(dot(p,vec2f(12.9898,4.1414)))*43758.5453);}
fn bg_bsl_stars(direction:vec3f,origin:vec3f,sun:vec3f,time:f32,rain:f32,moon_multiplier:f32)->vec3f {
    let up=clamp(direction.y,0.0,1.0);
    if up<=0.0 {return vec3f(0.0);}
    let plane=direction/(direction.y+length(direction.xz));
    var coordinates=plane.xz*0.4+origin.xz*0.0001+vec2f(time*0.00125,0.0);
    coordinates=floor(coordinates*1024.0)/1024.0;
    let visibility=clamp(-sun.y*10.0+0.5,0.0,1.0);
    let multiplier=sqrt(sqrt(up))*5.0*(1.0-clamp(rain,0.0,1.0))*visibility;
    let noise=bg_bsl_star_noise(coordinates)*bg_bsl_star_noise(coordinates+0.10)*bg_bsl_star_noise(coordinates+0.23);
    let star=clamp(noise-0.8125,0.0,1.0)*multiplier*smoothstep(-0.997,-0.992,dot(direction,sun));
    let night=vec3f(96.0,192.0,255.0)*(0.3/255.0)*moon_multiplier;
    return star*pow(night,vec3f(0.8));
}
