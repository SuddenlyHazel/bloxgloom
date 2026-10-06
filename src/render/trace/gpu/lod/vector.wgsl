// Same16-byte header,48-byte node and96-byte triangle records; offsets remain words.
@group(0) @binding(11) var<storage,read> ray_lod_page_0:array<vec4u>;
@group(0) @binding(12) var<storage,read> ray_lod_page_1:array<vec4u>;
@group(0) @binding(13) var<storage,read> ray_lod_page_2:array<vec4u>;
@group(0) @binding(14) var<storage,read> ray_lod_page_3:array<vec4u>;
fn ray_lod_quad(page:u32,offset:u32)->vec4u {
    let index=offset>>2u;
    switch page {
        case 0u: {return ray_lod_page_0[index];}
        case 1u: {return ray_lod_page_1[index];}
        case 2u: {return ray_lod_page_2[index];}
        case 3u: {return ray_lod_page_3[index];}
        default: {return vec4u(0u);}
    }
}
fn ray_lod_word(page:u32,offset:u32)->u32 {
    return ray_lod_quad(page,offset)[offset&3u];
}
fn ray_lod_float3(page:u32,offset:u32)->vec3f {
    return bitcast<vec3f>(ray_lod_quad(page,offset).xyz);
}
fn ray_lod_float4(page:u32,offset:u32)->vec4f {
    return bitcast<vec4f>(ray_lod_quad(page,offset));
}
fn ray_lod_node(page:u32,index:u32)->RayNode {
    let header=ray_lod_quad(page,0u);
    let offset=header.z+12u*index;
    let low=ray_lod_quad(page,offset);
    let high=ray_lod_quad(page,offset+4u);
    let links=ray_lod_quad(page,offset+8u);
    return RayNode(bitcast<vec3f>(low.xyz),low.w,bitcast<vec3f>(high.xyz),high.w,links.x,0u,0u,0u);
}
fn ray_lod_triangle(encoded:u32)->RayTriangle {
    let page=(encoded>>28u)-1u;let index=encoded&0x0fffffffu;
    if page>=4u {
        return RayTriangle(vec4f(0.0),vec4f(0.0),vec4f(0.0),vec4f(0.0),vec2f(0.0),0u,0u,vec4f(0.0));
    }
    let header=ray_lod_quad(page,0u);
    if index>=header.y {
        return RayTriangle(vec4f(0.0),vec4f(0.0),vec4f(0.0),vec4f(0.0),vec2f(0.0),0u,0u,vec4f(0.0));
    }
    let offset=header.w+24u*index;
    let a=ray_lod_quad(page,offset);let b=ray_lod_quad(page,offset+4u);let c=ray_lod_quad(page,offset+8u);
    let uv_ab=ray_lod_quad(page,offset+12u);let metadata=ray_lod_quad(page,offset+16u);let normal=ray_lod_quad(page,offset+20u);
    return RayTriangle(bitcast<vec4f>(a),bitcast<vec4f>(b),bitcast<vec4f>(c),bitcast<vec4f>(uv_ab),
        bitcast<vec2f>(metadata.xy),metadata.z,metadata.w,bitcast<vec4f>(normal));
}

