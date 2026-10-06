struct BgUnderwaterFrame {inverse:mat4x4f,eye:vec4f,color:vec4f,options:vec4f};
@group(0) @binding(0) var scene:texture_2d<f32>;
@group(0) @binding(1) var depth:texture_depth_2d;
@group(0) @binding(2) var<uniform> frame:BgUnderwaterFrame;
@group(0) @binding(3) var sampler_linear:sampler;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position)vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[id],0.0,1.0);
}
fn bg_reference_underwater(color:vec3f,distance:f32,fog_color:vec3f)->vec3f {
 let amount=mix(0.35,1.0,1.0-exp(-2.0*distance/64.0));
 let mixed=mix(sqrt(max(color,vec3f(0.0))),sqrt(max(fog_color,vec3f(0.0))),amount);
 return mixed*mixed;
}
@fragment fn fog(@builtin(position) p:vec4f)->@location(0)vec4f {
 let pixel=vec2i(p.xy);let uv=p.xy/vec2f(textureDimensions(scene));
 let position=frame.inverse*vec4f(uv.x*2.0-1.0,1.0-uv.y*2.0,textureLoad(depth,pixel,0),1.0);
 let distance=length(position.xyz/position.w-frame.eye.xyz);
 return vec4f(bg_reference_underwater(textureLoad(scene,pixel,0).rgb,distance,frame.color.rgb),1.0);
}
@fragment fn copy(@builtin(position) p:vec4f)->@location(0)vec4f {return textureLoad(scene,vec2i(p.xy),0);}
