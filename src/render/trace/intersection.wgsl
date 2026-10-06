struct RayNode {low:vec3f,first:u32,high:vec3f,count:u32,escape:u32,pad0:u32,pad1:u32,pad2:u32};
struct RayTriangle {a:vec4f,b:vec4f,c:vec4f,uv_ab:vec4f,uv_c:vec4f,normal:vec4f};
struct RayHit {distance:f32,triangle:u32,uv:vec2f,normal:vec3f};
@group(0) @binding(1) var<storage,read> ray_nodes:array<RayNode>;
@group(0) @binding(2) var<storage,read> ray_triangles:array<RayTriangle>;
@group(1) @binding(0) var ray_albedo:texture_2d_array<f32>;
@group(1) @binding(1) var ray_sampler:sampler;
@group(1) @binding(2) var<storage,read> ray_emission:array<f32>;
@group(1) @binding(3) var ray_normals:texture_2d_array<f32>;
@group(1) @binding(4) var ray_specular:texture_2d_array<f32>;
struct RayMaterialMetadata {flags:u32,layer:u32};
@group(1) @binding(5) var<storage,read> ray_materials:array<RayMaterialMetadata>;
fn ray_wind(p:vec3f,n:vec3f,uv:vec2f,flags:u32)->vec3f {
    if (flags&64u)==0u {return p;}
    var wind_uv=uv;
    if (flags&8u)!=0u {wind_uv.y=select(0.5+0.5*uv.y,0.5*uv.y,(flags&128u)!=0u);}
    return bg_foliage_wind(p,n,wind_uv,ray_frame.eye.w);
}
fn ray_box(origin:vec3f,inverse:vec3f,node:RayNode,limit:f32)->bool {
    let a=(node.low-origin)*inverse;let b=(node.high-origin)*inverse;
    let low=min(a,b);let high=max(a,b);
    return max(max(max(low.x,low.y),low.z),0.0001)<=min(min(min(high.x,high.y),high.z),limit);
}
fn ray_cast(origin:vec3f,direction:vec3f,limit:f32)->RayHit {
    var hit=RayHit(limit,0xffffffffu,vec2f(0.0),vec3f(0.0));
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    var index=0u;
    loop {
        if index>=ray_frame.counts.x {break;}
        let node=ray_nodes[index];
        if !ray_box(origin,inverse,node,hit.distance) {index=node.escape;continue;}
        if node.count==0u {index++;continue;}
        for(var i=node.first;i<node.first+node.count;i++) {
            let t=ray_triangles[i];let flags=ray_materials[u32(t.a.w)].flags;
            var a=t.a.xyz;var b=t.b.xyz;var c=t.c.xyz;
            // Production buffers are deformed once by the compute pass. Raw
            // shader fixtures retain this path to check caster/ray agreement.
            if ray_frame.parameters.y<0.5 {
                a=ray_wind(a,t.normal.xyz,t.uv_ab.xy,flags);
                b=ray_wind(b,t.normal.xyz,t.uv_ab.zw,flags);
                c=ray_wind(c,t.normal.xyz,t.uv_c.xy,flags);
            }
            let e1=b-a;let e2=c-a;let p=cross(direction,e2);let determinant=dot(e1,p);
            if abs(determinant)<0.0000001 {continue;}
            let inverse_det=1.0/determinant;let offset=origin-a;
            let u=dot(offset,p)*inverse_det;
            if u<0.0||u>1.0 {continue;}
            let q=cross(offset,e1);let v=dot(direction,q)*inverse_det;
            if v<0.0||u+v>1.0 {continue;}
            let distance=dot(e2,q)*inverse_det;
            if distance<=0.003||distance>=hit.distance {continue;}
            let uv=t.uv_ab.xy*(1.0-u-v)+t.uv_ab.zw*u+t.uv_c.xy*v;
            if t.c.w>0.5 && textureSampleLevel(ray_albedo,ray_sampler,uv,i32(ray_materials[u32(t.a.w)].layer),0.0).a<0.5 {continue;}
            var normal=normalize(cross(e1,e2));
            if dot(normal,direction)>0.0 {normal=-normal;}
            hit=RayHit(distance,i,uv,normal);
        }
        index=node.escape;
    }
    return hit;
}

// Visibility only: an accepted nonbotanical hit anywhere on the finite sun
// segment is opaque. Botanical sheets retain the original nearest/ordered
// transmission path; this helper never guesses their optical allocation.
fn ray_any_opaque(origin:vec3f,direction:vec3f,limit:f32)->bool {
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    var index=0u;
    loop {
        if index>=ray_frame.counts.x {break;}
        let node=ray_nodes[index];
        if !ray_box(origin,inverse,node,limit) {index=node.escape;continue;}
        if node.count==0u {index++;continue;}
        for(var i=node.first;i<node.first+node.count;i++) {
            let t=ray_triangles[i];let metadata=ray_materials[u32(t.a.w)];
            if (metadata.flags&64u)!=0u {continue;}
            // Only botanicals deform, so this opaque subset needs no wind.
            let e1=t.b.xyz-t.a.xyz;let e2=t.c.xyz-t.a.xyz;
            let p=cross(direction,e2);let determinant=dot(e1,p);
            if abs(determinant)<0.0000001 {continue;}
            let inverse_det=1.0/determinant;let offset=origin-t.a.xyz;
            let u=dot(offset,p)*inverse_det;
            if u<0.0||u>1.0 {continue;}
            let q=cross(offset,e1);let v=dot(direction,q)*inverse_det;
            if v<0.0||u+v>1.0 {continue;}
            let distance=dot(e2,q)*inverse_det;
            if distance<=0.003||distance>=limit {continue;}
            let uv=t.uv_ab.xy*(1.0-u-v)+t.uv_ab.zw*u+t.uv_c.xy*v;
            if t.c.w>0.5 && textureSampleLevel(ray_albedo,ray_sampler,uv,i32(metadata.layer),0.0).a<0.5 {continue;}
            return true;
        }
        index=node.escape;
    }
    return false;
}
