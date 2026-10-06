fn bg_reference_water_height(world:vec3f,offset:vec2f)->f32 {
    if water_reference.properties.x<0.5 {return 0.0;}
    let coordinate=world.xz+world.y*0.2;
    let wind=vec2f(water_reference.sky.climate.w*0.5);
    let a=textureSampleLevel(bg_reference_noise,bg_reference_noise_sampler,(coordinate-wind)/256.0+offset/256.0,0.0).g;
    let b=textureSampleLevel(bg_reference_noise,bg_reference_noise_sampler,(coordinate+wind)/48.0+offset/256.0,0.0).g;
    return mix(a,b,0.25);
}
fn bg_reference_water_normal(world:vec3f,relative:vec3f,normal:vec3f)->vec3f {
    var tangent=vec3f(1.0,0.0,0.0);var bitangent=vec3f(0.0,0.0,1.0);
    if abs(normal.y)<0.5 {tangent=normalize(cross(vec3f(0.0,1.0,0.0),normal));bitangent=vec3f(0.0,1.0,0.0);}
    let distance=max(length(relative),0.00001);
    let view=vec2f(dot(relative,tangent),dot(relative,bitangent))/distance;
    var position=world;
    for(var i=0u;i<4u;i++) {
        let height=-1.25*bg_reference_water_height(position,vec2f(0.0))+0.25;
        position.x+=height*view.x;position.z+=height*view.y;
    }
    let fresnel=pow(clamp(1.0+dot(normal,relative/distance),0.0,1.0),8.0);
    let strength=0.35*(1.0-fresnel);
    let x=(bg_reference_water_height(position,vec2f(-0.2,0.0))-bg_reference_water_height(position,vec2f(0.2,0.0)))/0.2;
    let y=(bg_reference_water_height(position,vec2f(0.0,-0.2))-bg_reference_water_height(position,vec2f(0.0,0.2)))/0.2;
    let mapped=vec3f(x,y,1.0-x*x-y*y)*strength+vec3f(0.0,0.0,1.0-strength);
    return normalize(tangent*mapped.x+bitangent*mapped.y+normal*mapped.z);
}
fn bg_reference_water_surface(normal:vec3f,sky:f32,glow:f32,world:vec3f,relative:vec3f,front:bool,shadow:f32,pixel:vec2f)->BgSceneOutput {
    var geometric=normal;if !front {geometric=-geometric;}
    let n=bg_reference_water_normal(world,relative,geometric);
    let view=-relative/max(length(relative),0.00001);
    let frame=bg_bsl_reference_frame();
    let base=pow(vec3f(64.0,160.0,255.0)*(0.35/255.0),vec3f(2.0))*0.1225;
    let diffuse=bg_bsl_default_surface(base,n,view,bg_bsl_reference_relative_lightmap(vec2f(glow,sky),relative),1.0,1.0,0.0,shadow,frame);
    let eye_water=water_reference.properties.y;
    let fresnel=(pow(clamp(1.0-dot(n,view),0.0,1.0),5.0)*0.98+0.02)*max(1.0-eye_water*0.5,0.5);
    let alpha=mix(0.7,1.0,fresnel);
    let reflected=reflect(-view,n);
    let screen_reflection=bg_water_source_reflection(world,n);
    var environment=bg_bsl_sky_default(reflected,water_reference.sky.sun.xyz,water_reference.sky.sun_radiance.w,water_reference.sky.climate.x,water_reference.sky.climate.y);
    if water_reference.properties.x>0.5 {
        let cloud=bg_reference_cloud_integrate_case(world,reflected,pixel,water_reference.sky,water_reference.sky.eye.y,true);
        environment=environment*cloud.a+cloud.rgb;
    }
    environment*=sky*sky*clamp(1.0-eye_water,0.0,1.0);
    let full_shadow=bg_bsl_default_scene_light(n,view,sky,1.0,shadow,frame).a;
    let highlight=bg_water_source_specular(n,view,sky,full_shadow);
    let reflection=mix(environment,screen_reflection.rgb,screen_reflection.a)+highlight*(1.0-screen_reflection.a)/max(pow(alpha,2.2)*fresnel,0.00000001);
    let color=mix(diffuse,reflection,fresnel);
    var result=bg_scene_output(color,vec3f(0.0),relative+camera.eye.xyz,max(sky,camera.eye.w),-2.0);
    result.color=vec4f(sqrt(max(result.color.rgb,vec3f(0.0))),alpha);
    // Source colortex1 translucency for reference shafts, before lighting/Fresnel.
    // Reference GI is disabled; keep the explicit reactive water classification.
    result.indirect=vec4f(mix(vec3f(1.0),sqrt(base),sqrt(0.7))*(1.0-pow(0.7,64.0)),-2.0);
    // Source SSR is resolved here before private sqrt blending; post-water
    // enhanced SSR must not apply a second replacement in linear scene space.
    result.reflection_normal=vec4f(0.0);result.reflection_response=vec4f(0.0);
    return result;
}
// Source WAVING_WATER default; lower/full voxel boundary vertices stay fixed.
fn bg_reference_water_wave(world:vec3f)->f32 {
    if fract(world.y+0.005)<=0.01 {return 0.0;}
    let time=water_reference.sky.climate.w;
    return (sin(6.2831854*(time*0.7+world.x*0.14+world.z*0.07))
        +sin(6.2831854*(time*0.5+world.x*0.10+world.z*0.20)))*0.0125;
}
