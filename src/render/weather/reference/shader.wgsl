struct Camera { view_projection:mat4x4f,sun:vec4f,horizon:vec4f,eye:vec4f,fog_range:vec4f,parallax:vec4f,sun_radiance:vec4f,sky_zenith:vec4f,ambient_lower:vec4f,ambient_upper:vec4f,cloud:vec4f,ambient_sh:array<vec4f,6> };
@group(0) @binding(0) var<uniform> camera:Camera;
struct Input {@location(0) position:vec3f,@location(1) uv:vec2f,@location(2) color:vec4f,@location(3) glow:f32};
struct Output {@builtin(position) clip:vec4f,@location(0) uv:vec2f,@location(1) color:vec4f,@location(2) glow:f32,@location(3) world:vec3f};
@vertex fn vs(v:Input)->Output {return Output(camera.view_projection*vec4f(v.position,1.0),v.uv,v.color,v.glow,v.position);}
// Native procedural rain replaces source texture artwork. Recover its encoded
// RGB before the unchanged source transfer. This is not Minecraft precipitation art.
fn bg_reference_rain_encoded(linear:vec3f)->vec3f {
 let positive=max(linear,vec3f(0.0));
 return max(select(1.055*pow(positive,vec3f(1.0/2.4))-0.055,positive*12.92,positive<=vec3f(0.0031308)),vec3f(0.0));
}
fn bg_reference_rain_color(encoded:vec3f,alpha:f32,rain:f32,block:f32,ambient:vec3f)->vec4f {
 let coverage=alpha*0.35*rain*length(encoded/3.0)*select(0.0,1.0,alpha>0.1)*1.4;
 let palette=vec3f(255.0,212.0,160.0)*(0.85/255.0);
 let lit=sqrt(max(encoded,vec3f(0.0)))*(ambient+block*block*palette*palette);
 return vec4f(sqrt(max(lit,vec3f(0.0))),coverage);
}
@fragment fn fs(v:Output)->@location(0) vec4f {
 let alpha=v.color.a*(1.0-smoothstep(0.45,1.0,v.uv.y));
 if alpha<=0.001 {discard;}
 let lm=bg_bsl_handlight(vec2f(v.glow,0.0),v.world-camera.eye.xyz,camera.ambient_sh[5].xyz,camera.ambient_sh[5].w);
 return bg_reference_rain_color(bg_reference_rain_encoded(v.color.rgb),alpha,camera.sun_radiance.w,lm.x,camera.ambient_upper.xyz);
}
