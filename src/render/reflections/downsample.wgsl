@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var opaque_depth: texture_depth_2d;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
@fragment fn downsample(@builtin(position) p:vec4f)->@location(0) vec4f {
 let edge=vec2i(textureDimensions(source))-1;let q=vec2i(p.xy)*2;
 return (textureLoad(source,min(q,edge),0)+textureLoad(source,min(q+vec2i(1,0),edge),0)
 +textureLoad(source,min(q+vec2i(0,1),edge),0)+textureLoad(source,min(q+vec2i(1,1),edge),0))*0.25;
}

// Sky radiance remains the analytic material fallback. It must not leak into
// rough scene cones through invalid/background pixels in this pyramid.
@fragment fn capture(@builtin(position) p:vec4f)->@location(0) vec4f {
 let q=vec2i(p.xy);if textureLoad(opaque_depth,q,0)>=0.999999 {return vec4f(0.0);}
 return vec4f(textureLoad(source,q,0).rgb,1.0);
}
