// Only water's private target uses source ALPHA_BLEND=0 encoding.
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var opaque_depth:texture_depth_2d;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
struct Encoded { @location(0) color:vec4f,@location(1) reflection:vec4f };
@fragment fn encode(@builtin(position) pixel:vec4f)->Encoded {
 let color=textureLoad(source,vec2i(pixel.xy),0);let depth=textureLoad(opaque_depth,vec2i(pixel.xy),0);
 return Encoded(vec4f(sqrt(max(color.rgb,vec3f(0.0))),color.a),vec4f(pow(max(color.rgb,vec3f(0.0)),vec3f(0.125))*0.5,select(0.0,1.0,depth<1.0)));
}
@fragment fn copy_depth(@builtin(position) pixel:vec4f)->@builtin(frag_depth) f32 {return textureLoad(opaque_depth,vec2i(pixel.xy),0);}
@fragment fn decode(@builtin(position) pixel:vec4f)->@location(0) vec4f {
 let color=textureLoad(source,vec2i(pixel.xy),0);return vec4f(color.rgb*color.rgb,color.a);
}
