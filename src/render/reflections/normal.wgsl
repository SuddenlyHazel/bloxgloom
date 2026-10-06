// Octahedral normals leave room for perceptual roughness and receiving depth
// in one RGBA16F target; receivers need not write opaque intersection depth.
fn bg_reflection_oct_encode(normal:vec3f)->vec2f {
 let n=normal/(abs(normal.x)+abs(normal.y)+abs(normal.z));
 if n.z>=0.0 {return n.xy;}
 return (vec2f(1.0)-abs(n.yx))*select(vec2f(-1.0),vec2f(1.0),n.xy>=vec2f(0.0));
}
fn bg_reflection_oct_decode(encoded:vec2f)->vec3f {
 var n=vec3f(encoded,1.0-abs(encoded.x)-abs(encoded.y));
 let t=max(-n.z,0.0);n.x+=select(t,-t,n.x>=0.0);n.y+=select(t,-t,n.y>=0.0);
 return normalize(n);
}
