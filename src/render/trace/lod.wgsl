// Four bounded static pages. Their summaries do not certify loaded air/sky.
// Header words: node count, triangle count, node offset, triangle offset.
const RAY_LOD_ROOT_ORDER:bool=false;
@group(0) @binding(11) var<storage,read> ray_lod_page_0:array<u32>;
@group(0) @binding(12) var<storage,read> ray_lod_page_1:array<u32>;
@group(0) @binding(13) var<storage,read> ray_lod_page_2:array<u32>;
@group(0) @binding(14) var<storage,read> ray_lod_page_3:array<u32>;
fn ray_lod_word(page:u32,offset:u32)->u32 {
    switch page {
        case 0u: {return ray_lod_page_0[offset];}
        case 1u: {return ray_lod_page_1[offset];}
        case 2u: {return ray_lod_page_2[offset];}
        case 3u: {return ray_lod_page_3[offset];}
        default: {return 0u;}
    }
}
fn ray_lod_float3(page:u32,offset:u32)->vec3f {
    return bitcast<vec3f>(vec3u(ray_lod_word(page,offset),ray_lod_word(page,offset+1u),ray_lod_word(page,offset+2u)));
}
fn ray_lod_float4(page:u32,offset:u32)->vec4f {
    return bitcast<vec4f>(vec4u(ray_lod_word(page,offset),ray_lod_word(page,offset+1u),ray_lod_word(page,offset+2u),ray_lod_word(page,offset+3u)));
}
fn ray_lod_node(page:u32,index:u32)->RayNode {
    let offset=ray_lod_word(page,2u)+12u*index;
    return RayNode(ray_lod_float3(page,offset),ray_lod_word(page,offset+3u),
        ray_lod_float3(page,offset+4u),ray_lod_word(page,offset+7u),ray_lod_word(page,offset+8u),0u,0u,0u);
}
fn ray_lod_triangle(encoded:u32)->RayTriangle {
    let page=(encoded>>28u)-1u;let index=encoded&0x0fffffffu;
    if page>=4u || index>=ray_lod_word(page,1u) {
        return RayTriangle(vec4f(0.0),vec4f(0.0),vec4f(0.0),vec4f(0.0),vec2f(0.0),0u,0u,vec4f(0.0));
    }
    let offset=ray_lod_word(page,3u)+24u*index;
    return RayTriangle(ray_lod_float4(page,offset),ray_lod_float4(page,offset+4u),ray_lod_float4(page,offset+8u),
        ray_lod_float4(page,offset+12u),bitcast<vec2f>(vec2u(ray_lod_word(page,offset+16u),ray_lod_word(page,offset+17u))),
        ray_lod_word(page,offset+18u),ray_lod_word(page,offset+19u),ray_lod_float4(page,offset+20u));
}
// Static LOD candidates load only nine position words until geometric
// acceptance. UVs/materials and saved-cell clipping retain the shared contract.
fn ray_lod_candidate(origin:vec3f,direction:vec3f,limit:f32,i:u32,current:RayHit)->RayHit {
    let page=(i>>28u)-1u;let index=i&0x0fffffffu;
    if page>=4u || index>=ray_lod_word(page,1u) {return current;}
    let base=ray_lod_word(page,3u)+24u*index;
    let a=ray_lod_float3(page,base);
    let b=ray_lod_float3(page,base+4u);
    let c=ray_lod_float3(page,base+8u);
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
    let flags=ray_lod_word(page,base+19u);
    if (flags&1u)!=0u {
        let outward=ray_lod_float3(page,base+20u);
        let cell=vec3i(floor((origin+direction*distance-outward*0.002)/16.0));
        if ray_loaded_cell(cell) {return current;}
    }
    let ab=ray_lod_float4(page,base+12u);
    let uv_c=bitcast<vec2f>(vec2u(ray_lod_word(page,base+16u),ray_lod_word(page,base+17u)));
    let uv=ab.xy*(1.0-u-v)+ab.zw*u+uv_c*v;
    let cutout=bitcast<f32>(ray_lod_word(page,base+11u));
    if cutout>0.5 && ((flags&1u)==0u||(flags&8u)!=0u) {
        let material=u32(bitcast<f32>(ray_lod_word(page,base+3u)));
        if textureSampleLevel(ray_albedo,ray_sampler,uv,i32(ray_materials[material].layer),0.0).a<0.5 {return current;}
    }
    var normal=normalize(cross(e1,e2));
    if dot(normal,direction)>0.0 {normal=-normal;}
    return RayHit(distance,i,uv,normal);
}
fn ray_lod_cast_page(origin:vec3f,direction:vec3f,inverse:vec3f,limit:f32,current:RayHit,page:u32)->RayHit {
    var hit=current;
        let nodes=ray_lod_word(page,0u);var index=0u;
        loop {
            if index>=nodes {break;}
            let node=ray_lod_node(page,index);
            if !ray_box(origin,inverse,node,hit.distance) {index=node.escape;continue;}
            if node.count==0u {index++;continue;}
            for(var i=node.first;i<node.first+node.count;i++) {
                let encoded=((page+1u)<<28u)|i;
                if RAY_LOD_TIERED {hit=ray_lod_candidate(origin,direction,limit,encoded,hit);}
                else {hit=ray_test_triangle(origin,direction,limit,encoded,hit,ray_lod_triangle(encoded));}
            }
            index=node.escape;
        }
    return hit;
}
fn ray_lod_cast(origin:vec3f,direction:vec3f,limit:f32,current:RayHit)->RayHit {
    var hit=current;
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    if !RAY_LOD_ROOT_ORDER {
        for(var page=0u;page<4u;page++) {hit=ray_lod_cast_page(origin,direction,inverse,limit,hit,page);}
        return hit;
    }
    // Four root entries only: no new traversal stack or per-node order change.
    // Triangle acceptance retains the strict smaller-ID exact-distance rule.
    var pages:array<RayPending,4>;
    for(var page=0u;page<4u;page++) {
        var near=1e30;
        if ray_lod_word(page,0u)>0u {
            let entry=ray_box_near(origin,inverse,ray_lod_node(page,0u),hit.distance);
            if entry>=0.0 {near=entry;}
        }
        pages[page]=RayPending(page,near);
    }
    for(var i=1u;i<4u;i++) {
        let entry=pages[i];var j=i;
        while j>0u {
            if pages[j-1u].near<=entry.near {break;}
            pages[j]=pages[j-1u];j--;
        }
        pages[j]=entry;
    }
    for(var i=0u;i<4u;i++) {
        // Equality remains eligible because a later, smaller triangle ID may
        // win an exact tie. A root miss cannot become eligible as the limit falls.
        if pages[i].near>hit.distance {continue;}
        hit=ray_lod_cast_page(origin,direction,inverse,limit,hit,pages[i].index);
    }
    return hit;
}
// Only accepted nonbotanical opaque surfaces can short-circuit sunlight.
// Water and botanical sheets keep the shared ordered transmission path.
fn ray_lod_any_opaque(origin:vec3f,direction:vec3f,limit:f32)->bool {
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    for(var page=0u;page<4u;page++) {
        let nodes=ray_lod_word(page,0u);var index=0u;
        loop {
            if index>=nodes {break;}
            let node=ray_lod_node(page,index);
            if !ray_box(origin,inverse,node,limit) {index=node.escape;continue;}
            if node.count==0u {index++;continue;}
            for(var i=node.first;i<node.first+node.count;i++) {
                let encoded=((page+1u)<<28u)|i;
                let base=ray_lod_word(page,3u)+24u*i;
                let flags=ray_lod_word(page,base+19u);
                if (flags&2u)!=0u {continue;}
                if (flags&8u)!=0u {
                    let material=u32(bitcast<f32>(ray_lod_word(page,base+3u)));
                    if (ray_materials[material].flags&64u)!=0u {continue;}
                }
                var hit=RayHit(limit,0xffffffffu,vec2f(0.0),vec3f(0.0));
                if RAY_LOD_TIERED {hit=ray_lod_candidate(origin,direction,limit,encoded,hit);}
                else {hit=ray_test_triangle(origin,direction,limit,encoded,hit,ray_lod_triangle(encoded));}
                if hit.triangle!=0xffffffffu {return true;}
            }
            index=node.escape;
        }
    }
    return false;
}
