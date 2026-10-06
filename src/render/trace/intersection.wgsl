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
// One accepted triangle path shared by the reference and near-first walks.
// Smaller triangle IDs win exact ties, matching contiguous stackless DFS order.
fn ray_triangle_hit(origin:vec3f,direction:vec3f,limit:f32,i:u32,current:RayHit)->RayHit {
    let t=ray_triangles[i];var flags=0u;
    if !RAY_MATERIAL_FAST {flags=ray_materials[u32(t.a.w)].flags;}
    var a=t.a.xyz;var b=t.b.xyz;var c=t.c.xyz;
    if ray_frame.parameters.y<0.5 {
        if RAY_MATERIAL_FAST {flags=ray_materials[u32(t.a.w)].flags;}
        a=ray_wind(a,t.normal.xyz,t.uv_ab.xy,flags);
        b=ray_wind(b,t.normal.xyz,t.uv_ab.zw,flags);
        c=ray_wind(c,t.normal.xyz,t.uv_c.xy,flags);
    }
    let e1=b-a;let e2=c-a;let p=cross(direction,e2);let determinant=dot(e1,p);
    if abs(determinant)<0.0000001 {return current;}
    let inverse_det=1.0/determinant;let offset=origin-a;
    let u=dot(offset,p)*inverse_det;
    if u<0.0||u>1.0 {return current;}
    let q=cross(offset,e1);let v=dot(direction,q)*inverse_det;
    if v<0.0||u+v>1.0 {return current;}
    let distance=dot(e2,q)*inverse_det;
    if distance<=0.003||distance>=limit||distance>current.distance {return current;}
    if distance==current.distance&&i>=current.triangle {return current;}
    let uv=t.uv_ab.xy*(1.0-u-v)+t.uv_ab.zw*u+t.uv_c.xy*v;
    if t.c.w>0.5 && textureSampleLevel(ray_albedo,ray_sampler,uv,i32(ray_materials[u32(t.a.w)].layer),0.0).a<0.5 {return current;}
    var normal=normalize(cross(e1,e2));
    if dot(normal,direction)>0.0 {normal=-normal;}
    return RayHit(distance,i,uv,normal);
}
fn ray_cast_stackless(origin:vec3f,direction:vec3f,limit:f32)->RayHit {
    var hit=RayHit(limit,0xffffffffu,vec2f(0.0),vec3f(0.0));
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    var index=0u;
    loop {
        if index>=ray_frame.counts.x {break;}
        let node=ray_nodes[index];
        if !ray_box(origin,inverse,node,hit.distance) {index=node.escape;continue;}
        if node.count==0u {index++;continue;}
        for(var i=node.first;i<node.first+node.count;i++) {
            hit=ray_triangle_hit(origin,direction,limit,i,hit);
        }
        index=node.escape;
    }
    return hit;
}
fn ray_box_near(origin:vec3f,inverse:vec3f,node:RayNode,limit:f32)->f32 {
    let a=(node.low-origin)*inverse;let b=(node.high-origin)*inverse;
    let low=min(a,b);let high=max(a,b);
    let near=max(max(max(low.x,low.y),low.z),0.0001);
    let far=min(min(min(high.x,high.y),high.z),limit);
    return select(-1.0,near,near<=far);
}
struct RayPending {index:u32,near:f32};
fn ray_cast_near_first(origin:vec3f,direction:vec3f,limit:f32)->RayHit {
    var hit=RayHit(limit,0xffffffffu,vec2f(0.0),vec3f(0.0));
    if ray_frame.counts.x==0u {return hit;}
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    let root=ray_box_near(origin,inverse,ray_nodes[0],hit.distance);
    if root<0.0 {return hit;}
    var pending:array<RayPending,16>;
    var length=0u;var index=0u;var near=root;
    loop {
        if near<=hit.distance {
            let node=ray_nodes[index];
            if node.count==0u {
                let left=index+1u;let right=ray_nodes[left].escape;
                let a=ray_box_near(origin,inverse,ray_nodes[left],hit.distance);
                let b=ray_box_near(origin,inverse,ray_nodes[right],hit.distance);
                if a>=0.0&&b>=0.0 {
                    // Restart the original traversal on overflow; never drop a
                    // pending subtree, including exact equal-distance hits.
                    if length==16u {return ray_cast_stackless(origin,direction,limit);}
                    let left_first=a<=b;
                    pending[length]=RayPending(select(left,right,left_first),select(a,b,left_first));
                    length++;
                    index=select(right,left,left_first);near=select(b,a,left_first);
                    continue;
                }
                if a>=0.0 {index=left;near=a;continue;}
                if b>=0.0 {index=right;near=b;continue;}
            } else {
                for(var i=node.first;i<node.first+node.count;i++) {
                    hit=ray_triangle_hit(origin,direction,limit,i,hit);
                }
            }
        }
        if length==0u {break;}
        length--;index=pending[length].index;near=pending[length].near;
    }
    return hit;
}
fn ray_cast(origin:vec3f,direction:vec3f,limit:f32)->RayHit {
    if RAY_NEAR_FIRST {return ray_cast_near_first(origin,direction,limit);}
    return ray_cast_stackless(origin,direction,limit);
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
