@group(0) @binding(0) var<uniform> camera:Camera;
@group(0) @binding(1) var<storage,read_write> result:array<vec4f>;
struct ReflectionSettings {inverse:mat4x4f,projection:mat4x4f,eye:vec4f,horizon:vec4f,zenith:vec4f,size:vec4f,sun:vec4f,climate:vec4f};
@group(0) @binding(2) var<uniform> settings:ReflectionSettings;
struct RayFrame {inverse:mat4x4f,previous:mat4x4f,eye:vec4f,sun:vec4f,solar:vec4f,horizon:vec4f,zenith:vec4f,cloud:vec4f,climate:vec4f,parameters:vec4f,previous_eye:vec4f,counts:vec4u};
@group(0) @binding(3) var<uniform> ray_frame:RayFrame;
struct VertexOutput {world_position:vec3f,sky_level:f32,local_radiance:vec3f,local_direction:vec3f,indirect_bounce:vec4f};
struct BgSurface {normal:vec3f};
// Direct/local/fog are excluded solely by these fixture hooks; the actual
// material, water, fallback replacement and BRDF code is loaded verbatim.
fn bg_sun_radiance()->vec3f {return camera.sun_radiance.xyz;}
fn bg_shadowed_local_light(p:vec3f,n:vec3f,r:vec3f,d:vec3f)->vec3f {return vec3f(0.0);}
fn bg_local_material_radiance(r:vec3f,b:vec3f,g:vec3f,s:f32)->vec3f {return vec3f(0.0);}
fn bg_surface_light(n:vec3f,s:vec4f,sky:f32,l:vec3f,b:vec3f,g:vec3f,v:f32)->vec3f {return vec3f(0.0);}
fn bg_direct_light(n:vec3f,s:vec4f,sky:f32)->vec3f {return vec3f(0.0);}
fn bg_primary_sun_transmittance(p:vec3f)->f32 {return 1.0;}
fn bg_fog_transmittance(p:vec3f,s:f32)->f32 {return 1.0;}
fn bg_apply_fog(c:vec3f,p:vec3f,s:f32)->vec3f {return c;}
fn oracle(reflected:vec3f,roughness:f32)->vec3f {
    if BG_FOG_REFERENCE {return bg_pbr_prefiltered_sky(reflected,roughness,camera.horizon.xyz,camera.sky_zenith);}
    var t=vec3f(1.0,0.0,0.0);
    if abs(reflected.y)<0.999999 {t=normalize(vec3f(0.0,1.0,0.0)-reflected*reflected.y);}
    let b=cross(reflected,t);var sum=vec3f(0.0);var weight=0.0;
    for(var i=0u;i<8u;i++) {
        let u=(f32(i)+0.5)/8.0;let a=roughness*roughness;
        let c=sqrt((1.0-u)/(1.0+(a*a-1.0)*u));let s=sqrt(max(0.0,1.0-c*c));
        let phi=f32(i)*2.399963229728653;
        let h=reflected*c+t*(s*cos(phi))+b*(s*sin(phi));
        let ray=2.0*dot(reflected,h)*h-reflected;
        let w=max(dot(reflected,ray),0.0);
        sum+=bg_bsl_sky_default(ray,camera.sun.xyz,clamp(camera.sun.y/BG_FOG_NOON_HEIGHT,0.0,1.0),camera.sun_radiance.w,camera.ambient_upper.w)
            *smoothstep(-0.08,0.0,ray.y)*camera.sky_zenith.w*w;weight+=w;
    }
    return sum/max(weight,0.00001);
}
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) id:vec3u) {
    let view=normalize(camera.eye.xyz);let n=vec3f(0.0,1.0,0.0);let reflected=reflect(-view,n);
    let pbr=bg_decode_pbr(vec4f(0.44,0.0,0.0,1.0),vec3f(0.5),false,true);
    let response=bg_pbr_material_environment_weight(dot(n,view),pbr);
    let expected=oracle(reflected,pbr.roughness)*response;
    let actual=bg_material_highlight(VertexOutput(vec3f(0.0),1.0,vec3f(0.0),vec3f(0.0),vec4f(0.0)),BgSurface(n),pbr,0.0,1.0);
    var value=vec3f(0.0);
    if id.x==0u {value=actual;}
    if id.x==1u {value=expected;}
    let water_view=vec3f(0.0,0.0,1.0);let water_n=water_view;
    if id.x==2u {value=bg_water_surface(vec4f(0.0,0.0,0.0,0.5),water_n,1.0,0.0,vec3f(0.0),-water_view,true,0.0,0.0,vec4f(0.0)).color.rgb;}
    if id.x==3u {value=oracle(water_view,0.12)*bg_pbr_environment_weight(1.0,0.12,vec3f(0.02));}
    if id.x==4u {value=bg_reflection_fallback(reflected,pbr.roughness,BG_FOG_REFERENCE)*response-expected;}
    if id.x==5u {value=ray_specular_fallback(reflected,pbr.roughness)*response-expected;}
    if id.x==6u && BG_FOG_REFERENCE {value=bg_reflection_fallback(reflected,pbr.roughness,false)-bg_bsl_artistic_environment(reflected,settings.sun,settings.climate.xy);}
    if id.x==7u {value=bg_prefiltered_environment(reflected,pbr.roughness,camera.horizon.xyz,camera.sky_zenith,settings.sun,settings.climate.xy,false)-bg_pbr_prefiltered_sky(reflected,pbr.roughness,camera.horizon.xyz,camera.sky_zenith);}
    if id.x==8u {value=bg_material_highlight(VertexOutput(vec3f(0.0),1.0,vec3f(0.0),vec3f(0.0),vec4f(0.0)),BgSurface(n),bg_bsl_default_material(),0.0,1.0);}
    if id.x==9u {value=actual*0.37-ray_specular_fallback(reflected,pbr.roughness)*response*0.37;}
    if id.x==10u {value=oracle(reflected,pbr.roughness);}
    if id.x==11u {value=bg_pbr_prefiltered_sky(reflected,pbr.roughness,camera.horizon.xyz,camera.sky_zenith);}
    result[id.x]=vec4f(value,1.0);
}
