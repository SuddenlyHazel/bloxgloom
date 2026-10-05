struct Camera { view_projection: mat4x4f, sun: vec4f, horizon: vec4f, eye: vec4f, fog_range: vec4f, parallax: vec4f, sun_radiance: vec4f, sky_zenith: vec4f, ambient_lower: vec4f, ambient_upper: vec4f };
@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<uniform> clock:vec4f;
struct Input { @location(0) position:vec3f,@location(1) normal:vec3f,@location(2) color:vec4f,@location(3) light:vec2f };
struct Output { @builtin(position) position:vec4f,@location(0) world:vec3f,@location(1) normal:vec3f,@location(2) color:vec4f,@location(3) light:vec2f };
@vertex fn vs(v:Input)->Output { return Output(camera.view_projection*vec4f(v.position,1.0),v.position,v.normal,v.color,v.light); }
struct WaterOutput { @location(0) color:vec4f,@location(1) indirect:vec4f };
@fragment fn fs(v:Output,@builtin(front_facing) front:bool)->WaterOutput {
    var n=v.normal;
    if abs(n.y)>0.5 {
        let ripple=vec2f(sin(v.world.x*1.4+v.world.z*0.7+clock.x*0.9),cos(v.world.z*1.7-v.world.x*0.4-clock.x*0.7))*0.045;
        n=normalize(n+vec3f(ripple.x,0.0,ripple.y));
    }
    if !front { n = -n; }
    let view=normalize(camera.eye.xyz-v.world);
    let fresnel=0.02+0.98*pow(1.0-max(dot(n,view),0.0),5.0);
    let light=bg_surface_light(n,camera.sun,v.light.x,v.light.y*v.light.y*vec3f(1.0,0.57,0.23),vec3f(0.0),vec3f(0.0),1.0);
    let reflection=bg_environment_radiance(reflect(-view,n))*v.light.x;
    let halfway=view+normalize(camera.sun.xyz);
    let half_vector=halfway/max(length(halfway),0.00001);
    let specular=pow(max(dot(n,half_vector),0.0),180.0)*bg_sun_radiance()*v.light.x*0.7;
    let color=bg_apply_fog(mix(v.color.rgb*light,reflection,fresnel)+specular,v.world,max(v.light.x,camera.eye.w));
    let alpha=clamp(v.color.a+fresnel*(1.0-v.color.a),0.05,0.96);
    return WaterOutput(vec4f(color,alpha),vec4f(0.0,0.0,0.0,-1.0));
}
