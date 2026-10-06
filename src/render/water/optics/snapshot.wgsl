@group(0) @binding(0) var color:texture_2d<f32>;
@group(0) @binding(1) var depth:texture_depth_2d;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
struct Background {@location(0) color:vec4f,@location(1) depth:f32};
@fragment fn fs(@builtin(position) pixel:vec4f)->Background {
 return Background(textureLoad(color,vec2i(pixel.xy),0),textureLoad(depth,vec2i(pixel.xy),0));
}
