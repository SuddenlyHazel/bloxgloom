@group(0) @binding(0) var scene:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));return vec4f(p[i],0,1);
}
@fragment fn encode(@builtin(position) p:vec4f)->@location(0) vec4f {
 let color=textureLoad(scene,vec2i(p.xy),0);return vec4f(sqrt(max(color.rgb,vec3f(0))),color.a);
}
@fragment fn decode(@builtin(position) p:vec4f)->@location(0) vec4f {
 let color=textureLoad(scene,vec2i(p.xy),0);return vec4f(color.rgb*color.rgb,color.a);
}
