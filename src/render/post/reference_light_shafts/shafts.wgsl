// Source Overworld lightShafts/composite1 defaults; reflected media are separate.
struct ShaftFrame {inverse:mat4x4f,shadow:mat4x4f,eye:vec4f,forward:vec4f,sun:vec4f,light:vec4f,parameters:vec4f,options:vec4f};
@group(0) @binding(0) var scene:texture_2d<f32>;
@group(0) @binding(1) var front:texture_depth_2d;
@group(0) @binding(2) var opaque:texture_depth_2d;
@group(0) @binding(3) var metadata:texture_2d<f32>;
@group(0) @binding(4) var noise:texture_2d<f32>;
@group(0) @binding(5) var noise_sampler:sampler;
@group(0) @binding(6) var<uniform> frame:ShaftFrame;
@group(0) @binding(7) var shadow_depth:texture_depth_2d;
@group(0) @binding(8) var shadow_sampler:sampler_comparison;
@group(0) @binding(9) var encoded:texture_2d<f32>;
@group(0) @binding(10) var linear_sampler:sampler;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position)vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[id],0.0,1.0);
}
fn bg_shaft_position(uv:vec2f,depth:f32)->vec3f {
 let p=frame.inverse*vec4f(uv.x*2.0-1.0,1.0-uv.y*2.0,depth,1.0);return p.xyz/p.w;
}
fn bg_shaft_stop(distance:f32,front_depth:f32,opaque_depth:f32,translucent:vec3f)->bool {
 return distance>=128.0 || distance>opaque_depth || (distance>front_depth&&all(translucent==vec3f(0.0)));
}
fn bg_shaft_water_tint()->vec3f {
 let palette=pow(vec3f(64.0,160.0,255.0)/255.0,vec3f(2.0))/0.1225;
 return mix(vec3f(1.0),palette,pow(0.7,0.25));
}
fn bg_shaft_shadow_tint(translucent:vec3f,distance:f32,front_depth:f32,water:f32,exterior:f32)->vec3f {
 if distance>front_depth {return translucent;}
 if water>0.5 {return bg_shaft_water_tint()*(distance/512.0)*(1.0+exterior);}
 return vec3f(1.0);
}
fn bg_shaft_encode(radiance:vec3f,dither:f32)->vec3f {
 var color=pow(radiance/32.0,vec3f(0.25));
 if dot(color,color)>0.0 {color+=(dither-0.25)/128.0;}
 return color;
}
@fragment fn integrate(@builtin(position) p:vec4f)->@location(0)vec4f {
 let size=vec2f(textureDimensions(front));let uv=p.xy/size;let pixel=vec2i(p.xy);
 let endpoint=bg_shaft_position(uv,textureLoad(front,pixel,0));
 let behind=bg_shaft_position(uv,textureLoad(opaque,pixel,0));
 let relative=endpoint-frame.eye.xyz;let direction=normalize(relative);
 let linear_front=dot(relative,frame.forward.xyz);
 let linear_opaque=dot(behind-frame.eye.xyz,frame.forward.xyz);
 let material=textureLoad(metadata,pixel,0);
 let translucent=select(vec3f(0.0),material.rgb,material.a< -1.5);
 let gl_pixel=vec2f(p.x,size.y-p.y);
 let blue=textureSampleLevel(noise,noise_sampler,gl_pixel/512.0,0.0).b;
 let dither=fract(blue+frame.options.x*0.618);
 let visibility=frame.parameters.y;
 let falloff=bg_bsl_shaft_falloff(dot(direction,frame.sun.xyz),frame.sun.w,visibility,frame.parameters.x,frame.eye.w);
 var sum=vec3f(0.0);
 for(var i=0u;i<7u;i++) {
   let distance=exp2(f32(i)+dither)-0.95;
   if bg_shaft_stop(distance,linear_front,linear_opaque,translucent) {break;}
   let world=frame.eye.xyz+relative*(distance/linear_front);
   let projected=frame.shadow*vec4f(world,1.0);
   let shadow=bg_bsl_distort_shadow(vec3f(projected.xy,projected.z*2.0-1.0),frame.options.y);
   var sample=1.0;
   if length(shadow.xy*2.0-1.0)<1.0 && shadow.z<0.5 {
     sample=textureSampleCompareLevel(shadow_depth,shadow_sampler,vec2f(shadow.x,1.0-shadow.y),shadow.z+0.0512*frame.parameters.w);
     sample*=sample;
   }
   sum+=vec3f(sample)*bg_shaft_shadow_tint(translucent,distance,linear_front,frame.parameters.z,frame.eye.w);
 }
 let rain=1.0-frame.parameters.x*frame.eye.w*0.875;
 let bedrock=clamp((frame.eye.y+70.0)/8.0,0.0,1.0);
 let radiance=sum*falloff*frame.light.rgb*(0.25*rain*frame.light.w*bedrock/7.0);
 return vec4f(bg_shaft_encode(radiance,dither),1.0);
}
@fragment fn reconstruct(@builtin(position) p:vec4f)->@location(0)vec4f {
 let size=vec2f(textureDimensions(encoded));let uv=p.xy/size;
 // GLSL's +Y is up; source diagonal offsets are reflected into WGPU texture Y.
 let taps=array<vec2f,4>(vec2f(1.5,-0.5),vec2f(-0.5,-1.5),vec2f(-1.5,0.5),vec2f(0.5,1.5));
 var sum=vec3f(0.0);
 for(var i=0u;i<4u;i++) {sum+=textureSampleLevel(encoded,linear_sampler,uv+taps[i]/size,0.0).rgb;}
 let decoded=sum*sum*sum*sum*0.125;
 return vec4f(textureLoad(scene,vec2i(p.xy),0).rgb+decoded,1.0);
}
@fragment fn copy(@builtin(position) p:vec4f)->@location(0)vec4f {return textureLoad(scene,vec2i(p.xy),0);}
