struct ReflectionSettings {
 inverse:mat4x4f, projection:mat4x4f, eye:vec4f, horizon:vec4f, zenith:vec4f, size:vec4f,
};
@group(0) @binding(0) var depth:texture_depth_2d;
@group(0) @binding(1) var normals:texture_2d<f32>;
@group(0) @binding(2) var response:texture_2d<f32>;
@group(0) @binding(3) var radiance:texture_2d<f32>;
@group(0) @binding(4) var replacement:texture_2d<f32>;
@group(0) @binding(5) var<uniform> settings:ReflectionSettings;
@group(0) @binding(6) var linear_sampler:sampler;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
 let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
fn world(uv:vec2f,z:f32)->vec3f {
 let h=settings.inverse*vec4f(uv*vec2f(2.0,-2.0)+vec2f(-1.0,1.0),z,1.0);return h.xyz/h.w;
}
fn pixel(uv:vec2f)->vec2i {return clamp(vec2i(uv*settings.size.xy),vec2i(0),vec2i(settings.size.xy)-1);}
fn world_at(p:vec2i)->vec3f {
 let receiver=textureLoad(normals,p,0);let uv=(vec2f(p)+0.5)/settings.size.xy;
 if receiver.z<=0.0 {return world(uv,textureLoad(depth,p,0));}
 let origin=world(uv,0.0);let direction=normalize(world(uv,1.0)-origin);
 let offset=origin-settings.eye.xyz;let projected=dot(offset,direction);
 let travel=-projected+sqrt(max(0.0,receiver.w*receiver.w-dot(offset,offset)+projected*projected));
 return origin+direction*travel;
}
fn opaque_world_at(p:vec2i)->vec3f {return world((vec2f(p)+0.5)/settings.size.xy,textureLoad(depth,p,0));}
fn geometry_normal(p:vec2i,center:vec3f,opaque:bool)->vec3f {
 let edge=vec2i(settings.size.xy)-1;
 let lp=clamp(p-vec2i(1,0),vec2i(0),edge);let rp=clamp(p+vec2i(1,0),vec2i(0),edge);
 let up=clamp(p-vec2i(0,1),vec2i(0),edge);let dp=clamp(p+vec2i(0,1),vec2i(0),edge);
 let l=select(world_at(lp),opaque_world_at(lp),opaque);let r=select(world_at(rp),opaque_world_at(rp),opaque);
 let u=select(world_at(up),opaque_world_at(up),opaque);let d=select(world_at(dp),opaque_world_at(dp),opaque);
 let dx=select(r-center,center-l,distance(center,l)<distance(center,r));
 let dy=select(d-center,center-u,distance(center,u)<distance(center,d));
 let n=cross(dx,dy);let v=normalize(settings.eye.xyz-center);
 if dot(n,n)<0.000000001 {return v;}
 return normalize(n)*select(-1.0,1.0,dot(n,v)>=0.0);
}
fn projected(p:vec3f)->vec3f {
 let h=settings.projection*vec4f(p,1.0);
 return vec3f(h.xy/h.w*vec2f(0.5,-0.5)+0.5,h.z/h.w);
}
fn onscreen(p:vec3f)->bool {return all(p.xy>vec2f(0.002))&&all(p.xy<vec2f(0.998))&&p.z>0.0&&p.z<0.999999;}
@fragment fn trace(@builtin(position) frag:vec4f)->@location(0) vec4f {
 let p=min(vec2i(frag.xy)*2+vec2i(1),vec2i(settings.size.xy)-1);
 let data=textureLoad(normals,p,0);let weight=textureLoad(response,p,0);
 let z=data.w;
 if z<=0.0||data.z<=0.0||data.z>0.82||max(max(weight.r,weight.g),weight.b)<0.002 {return vec4f(0.0);}
 let center=world_at(p);let v=normalize(settings.eye.xyz-center);let n=bg_reflection_oct_decode(data.xy);
 let ray=reflect(-v,n);let gn=geometry_normal(p,center,false);
 // Mapped normals may face below the actual polygon. They cannot tunnel into
 // their own wall/floor and then collect light from its opposite side.
 if dot(ray,gn)<0.025 {return vec4f(0.0);}
 let origin=center+gn*(0.035+data.w*0.0008);
 let forward=normalize(world(vec2f(0.5),0.0)-settings.eye.xyz);
 var previous=0.0;var previous_gap=-1.0;var hit=vec2f(-1.0);var hit_distance=0.0;var candidates=0u;
 // Quadratic spacing bounds traversal at 32 blocks; only the crossing segment
 // gets binary refinement. Confidence fades before the screen/range limit.
 for(var i=1u;i<=48u;i++) {
  let travel=f32(i)*0.08+f32(i*i)*0.012;
  let point=origin+ray*travel;let screen=projected(point);
  if !onscreen(screen) {break;}
  let q=pixel(screen.xy);let scene_z=textureLoad(depth,q,0);
  if scene_z>=0.999999 {previous=travel;previous_gap=-1.0;continue;}
  let scene=world(screen.xy,scene_z);
  let gap=dot(point-scene,forward);
  if gap>0.0&&previous_gap<=0.0 {
   if candidates>=3u {break;}candidates+=1u;
   var lo=previous;var hi=travel;
   for(var refinement=0u;refinement<5u;refinement++) {
    let mid=(lo+hi)*0.5;let projected_mid=projected(origin+ray*mid);
    let q_mid=pixel(projected_mid.xy);let surface_point=world(projected_mid.xy,textureLoad(depth,q_mid,0));
    if dot(origin+ray*mid-surface_point,forward)>0.0 {hi=mid;}else{lo=mid;}
   }
   let candidate=projected(origin+ray*hi);let candidate_z=textureLoad(depth,pixel(candidate.xy),0);
   let difference=distance(origin+ray*hi,world(candidate.xy,candidate_z));
   let thickness=0.07+hi*0.006;
   if candidate_z<0.999999&&difference<thickness&&hi>0.14 {
    let hit_normal=geometry_normal(pixel(candidate.xy),world(candidate.xy,candidate_z),true);
    if dot(hit_normal,-ray)>0.05 {hit=candidate.xy;hit_distance=hi;break;}
   }
  }
  previous=travel;previous_gap=gap;
 }
 if hit.x<0.0 {return vec4f(0.0);}
 let edge=min(min(hit.x,1.0-hit.x),min(hit.y,1.0-hit.y));
 let confidence=smoothstep(0.005,0.07,edge)*(1.0-smoothstep(20.0,32.0,hit_distance));
 // Project the roughness cone into screen space; radiance is linear HDR at
 // every mip. Metals keep RGB reflectance instead of a scalar gloss strength.
 let derivative=length(world_at(min(p+vec2i(0,1),vec2i(settings.size.xy)-1))-center);
 let radius=data.z*data.z*hit_distance/max(derivative,0.0001)*0.3;
 let lod=clamp(log2(max(radius,1.0)),0.0,f32(textureNumLevels(radiance)-1u));
 let filtered=textureSampleLevel(radiance,linear_sampler,hit,lod);
 let scene=filtered.rgb/max(filtered.a,0.00001);
 let coverage=smoothstep(0.05,0.5,filtered.a);
 let fallback=bg_pbr_prefiltered_sky(ray,data.z,settings.horizon.xyz,settings.zenith)*weight.a;
 return vec4f((max(scene,vec3f(0.0))-fallback)*weight.rgb*confidence*coverage,confidence*coverage);
}
@fragment fn composite(@builtin(position) frag:vec4f)->@location(0) vec4f {
 let p=vec2i(frag.xy);let data=textureLoad(normals,p,0);
 if data.z<=0.0||data.z>0.82 {return vec4f(0.0);}
 let center=world_at(p);let n=bg_reflection_oct_decode(data.xy);let coordinate=(frag.xy-1.5)*0.5;let base=vec2i(floor(coordinate));
 let dimensions=vec2i(textureDimensions(replacement));var sum=vec3f(0.0);var weights=0.0;
 // Normal/plane-aware reconstruction avoids half-resolution reflection color
 // bleeding across silhouettes, neighboring faces, and roughness boundaries.
 for(var y=0;y<=1;y++){for(var x=0;x<=1;x++){
  let q=clamp(base+vec2i(x,y),vec2i(0),dimensions-1);
  let full=min(q*2+vec2i(1),vec2i(settings.size.xy)-1);
  let other=textureLoad(normals,full,0);
  if other.z<=0.0||other.w<=0.0 {continue;}
  let plane=abs(dot(world_at(full)-center,n));
  let d=abs(coordinate-vec2f(q));
  let w=max(0.0,1.0-d.x)*max(0.0,1.0-d.y)*pow(max(dot(n,bg_reflection_oct_decode(other.xy)),0.0),16.0)
    *exp(-plane*plane/0.0025)*exp(-abs(data.z-other.z)*12.0);
  sum+=textureLoad(replacement,q,0).rgb*w;weights+=w;
 }}
 return vec4f(sum/max(weights,0.00001),0.0);
}
