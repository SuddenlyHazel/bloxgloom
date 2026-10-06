fn source_celestial(sampled:vec4f,VoU:f32,is_moon:bool,sunVisibility:f32,phaseMult:f32)->vec3f {
    var albedo=sampled;
    albedo=vec4f(pow(albedo.rgb,vec3f(2.2))*albedo.a,albedo.a);
    var sunFade=smoothstep(0.0,1.0,1.0-pow(1.0-max(VoU*0.975+0.025,0.0),8.0));
    sunFade*=sunFade;
    albedo=vec4f(albedo.rgb*(1.50*1.50)*sunFade,albedo.a);
    if is_moon {
        let lightNight=vec3f(96.0,192.0,255.0)*1.00*(0.3*phaseMult)/255.0;
        let desat=dot(albedo.rgb,vec3f(0.299,0.587,0.114))*pow(lightNight,vec3f(1.6))*4.0;
        albedo=vec4f(mix(desat,albedo.rgb,sunVisibility),albedo.a);
    }
    let source_output=sqrt(max(albedo.rgb,vec3f(0.0)));
    return source_output*source_output;
}
fn source_star_fade(camera_y:f32,reference:bool)->f32 {
    if !reference {return 1.0;}
    let bedrockLevel=-64.0;
    return clamp((camera_y-bedrockLevel+6.0)/8.0,0.0,1.0);
}
