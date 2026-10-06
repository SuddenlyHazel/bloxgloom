// Checked-in lib/atmospherics/fog.glsl, Overworld defaults:
// density=1, NIGHT=4, WEATHER=1.5, INDOOR=1, height=62/falloff=7.
// FOG_INTERIOR is off; FAR_VANILLA_FOG=2 excludes the Overworld.
// Inputs are explicit: engine shelter is only a proxy for Minecraft's eBS.
fn bg_bsl_air_fog_amount(distance:f32,world_y:f32,exterior:f32,sun_sky_visibility:f32,rain:f32)->f32 {
    let clear_day=sun_sky_visibility*(1.0-rain);
    var optical=distance/1024.0;
    var density_mult=mix(1.0,1.5,rain)/mix(0.25,1.0,clear_day);
    density_mult=mix(1.0,density_mult*exterior,exterior);
    optical*=density_mult;
    let dampen=0.3*rain+0.5;
    optical=min(optical,(optical-dampen)*0.25+dampen);
    optical*=exp2(-max(world_y-62.0,0.0)/128.0);
    return 1.0-exp(-2.0*pow(optical,0.35*clear_day*exterior+1.25));
}

fn bg_bsl_air_fog_exterior(air:vec3f,exterior:f32,eye_y:f32,bedrock:f32)->vec3f {
    let minimum=pow(128.0*0.5/255.0,2.0)*0.04*0.5;
    // Iris bedrockLevel branch; engine explicitly supplies its own bedrock.
    return mix(vec3f(minimum),air*exterior,exterior)*clamp((eye_y-bedrock+6.0)/8.0,0.0,1.0);
}

fn bg_reference_fog_amount(world:vec3f)->f32 {
    let relative=world-camera.eye.xyz;
    var sun=camera.sun.xyz;
    sun=select(sun,-sun,camera.ambient_lower.w<0.5 && sun.y>0.0);
    // X is absolute eye Y even for floating-origin LOD; Y is bedrock level.
    let world_y=relative.y+camera.fog_range.x;
    return bg_bsl_air_fog_amount(length(relative),world_y,camera.eye.w,
        clamp(sun.y*2.0+0.5,0.0,1.0),camera.sun_radiance.w);
}
