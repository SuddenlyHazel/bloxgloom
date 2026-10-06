// A paired static replay retains the same RNG and excludes only current dynamic
// geometry. Every successful dynamic closest-hit query marks the dependency,
// including sunlight and medium visibility calls through ray_cast.
var<private> ray_dynamic_disabled:bool=false;
var<private> ray_dynamic_touched:bool=false;
fn dyn_wrap(value:f32,mode:u32)->f32 {
    if mode==0u {return clamp(value,0.0,1.0);}
    if mode==1u {return fract(value);}
    let v=value-floor(value*0.5)*2.0;return select(v,2.0-v,v>1.0);
}
fn dyn_texel(material:u32,uv:vec2f)->vec4f {
    let size=vec2u(dyn_assets[material+2u],dyn_assets[material+3u]);
    if size.x==0u||size.y==0u {return vec4f(1.0);}
    let wrapped=vec2f(dyn_wrap(uv.x,dyn_assets[material+8u]),dyn_wrap(uv.y,dyn_assets[material+9u]));
    let pixel=min(vec2u(floor(wrapped*vec2f(size))),size-vec2u(1));
    let rgba=unpack4x8unorm(dyn_assets[dyn_assets[material+1u]+pixel.y*size.x+pixel.x]);
    return vec4f(bg_actor_srgb(rgba.rgb),rgba.a);
}
fn dyn_color(triangle:u32,uv:vec2f)->vec4f {
    let t=8u+triangle*DYN_TRIANGLE_WORDS;let material=dyn_geometry[t+32u];let instance=dyn_geometry[t+33u];
    let kind=dyn_assets[material];var color=dyn_av4(material+4u);
    if kind==3u {
        let metadata=ray_materials[dyn_assets[material+14u]];
        color=textureSampleLevel(ray_albedo,ray_sampler,uv,i32(metadata.layer),0.0);
    } else if kind==0u&&dyn_frame[instance+7u]==0u {
        let cosmetics=dyn_frame[instance+32u];
        let c=vec3u(cosmetics&255u,(cosmetics>>8u)&255u,(cosmetics>>16u)&255u);
        color=vec4f(bg_actor_primitive_color(dyn_geometry[t+34u],c,color.rgb),color.a);
    } else if kind==1u {
        color*=dyn_texel(material,uv);
        let part=dyn_frame[instance+6u]+dyn_geometry[t+34u]*8u;
        color=vec4f(select(color.rgb*dyn_fv3(part),dyn_fv3(part),dyn_ff(part+3u)>0.5),color.a);
    } else if kind==2u {
        color=dyn_texel(material,uv);let surface=dyn_assets[material+12u];let group=dyn_assets[material+13u];
        let cosmetics=dyn_frame[instance+32u];
        if group>0u&&group<14u {
            color=bg_actor_hair_color(color,vec3f(unpack4x8unorm(dyn_frame[instance+35u]).rgb)*255.0,surface==8u);
        } else {
            let c=vec3u(cosmetics&255u,(cosmetics>>8u)&255u,(cosmetics>>16u)&255u);
            let i=dyn_frame[instance+34u];let iris=vec4u(i&255u,(i>>8u)&255u,(i>>16u)&255u,i>>24u);
            color=bg_actor_body_color(color,surface,c,iris);
        }
    }
    return vec4f(color.rgb*dyn_fv3(instance+28u),color.a);
}
// Alpha is tested before customization replacement, as in the GLB raster shader.
fn dyn_alpha(triangle:u32,uv:vec2f)->f32 {
    let t=8u+triangle*DYN_TRIANGLE_WORDS;let material=dyn_geometry[t+32u];let kind=dyn_assets[material];
    if kind==3u {let metadata=ray_materials[dyn_assets[material+14u]];
        return textureSampleLevel(ray_albedo,ray_sampler,uv,i32(metadata.layer),0.0).a;}
    return dyn_texel(material,uv).a*dyn_af(material+7u);
}
fn dyn_box(origin:vec3f,inverse:vec3f,index:u32,limit:f32)->bool {
    let at=dyn_geometry[4]+index*DYN_NODE_WORDS;let low=dyn_gv3(at);let high=dyn_gv3(at+4u);
    if any(high<low) {return false;}
    let a=(low-origin)*inverse;let b=(high-origin)*inverse;
    let near=min(a,b);let far=max(a,b);
    return max(max(max(near.x,near.y),near.z),0.0001)<=min(min(min(far.x,far.y),far.z),limit);
}
fn dyn_triangle(origin:vec3f,direction:vec3f,limit:f32,index:u32,current:RayHit)->RayHit {
    let t=8u+index*DYN_TRIANGLE_WORDS;if dyn_geometry[t+35u]==0u {return current;}
    let a=dyn_gv3(t);let e1=dyn_gv3(t+4u)-a;let e2=dyn_gv3(t+8u)-a;
    let p=cross(direction,e2);let det=dot(e1,p);let material=dyn_geometry[t+32u];
    if abs(det)<0.0000001 {return current;}
    if dyn_assets[material+11u]==0u&&det<=0.0 {return current;}
    let offset=origin-a;let u=dot(offset,p)/det;if u<0.0||u>1.0 {return current;}
    let q=cross(offset,e1);let v=dot(direction,q)/det;if v<0.0||u+v>1.0 {return current;}
    let distance=dot(e2,q)/det;let identity=index|0x80000000u;
    if distance<=0.003||distance>=limit||distance>current.distance {return current;}
    if distance==current.distance&&identity>=current.triangle {return current;}
    let ab=dyn_gv4(t+24u);let c=vec2f(dyn_gf(t+28u),dyn_gf(t+29u));
    let uv=ab.xy*(1.0-u-v)+ab.zw*u+c*v;
    let cutoff=dyn_af(material+10u);
    if cutoff>=0.0&&dyn_alpha(index,uv)<cutoff {return current;}
    var normal=dyn_gv3(t+12u)*(1.0-u-v)+dyn_gv3(t+16u)*u+dyn_gv3(t+20u)*v;
    if dot(normal,normal)<0.000000000001 {normal=cross(e1,e2);}
    normal=normalize(normal);if dot(normal,direction)>0.0 {normal=-normal;}
    return RayHit(distance,identity,uv,normal);
}
fn dynamic_ray_cast_impl(origin:vec3f,direction:vec3f,limit:f32,current:RayHit,primary:bool)->RayHit {
    var hit=current;if ray_dynamic_disabled||dyn_geometry[3]==0u {return hit;}
    let inverse=1.0/select(direction,vec3f(0.0000001),abs(direction)<vec3f(0.0000001));
    var node=0u;
    loop {if node>=dyn_geometry[3] {break;}let at=dyn_geometry[4]+node*DYN_NODE_WORDS;
        if !dyn_box(origin,inverse,node,hit.distance) {node=dyn_geometry[at+8u];continue;}
        if dyn_geometry[at+7u]==0u {node++;continue;}
        let instance=8u+dyn_geometry[at+3u]*DYN_INSTANCE_WORDS;
        if primary&&dyn_frame[instance+40u]!=0u {node=dyn_geometry[at+8u];continue;}
        var blas=dyn_frame[instance+2u];let end=blas+dyn_frame[instance+4u];
        loop {if blas>=end {break;}let leaf=dyn_geometry[4]+blas*DYN_NODE_WORDS;
            if !dyn_box(origin,inverse,blas,hit.distance) {blas=dyn_geometry[leaf+8u];continue;}
            if dyn_geometry[leaf+7u]==0u {blas++;continue;}
            let first=dyn_geometry[leaf+3u];
            for(var i=first;i<first+dyn_geometry[leaf+7u];i++) {hit=dyn_triangle(origin,direction,limit,i,hit);}
            blas=dyn_geometry[leaf+8u];
        }
        node=dyn_geometry[at+8u];
    }
    if hit.triangle!=current.triangle {ray_dynamic_touched=true;}
    return hit;
}
fn dynamic_ray_cast(origin:vec3f,direction:vec3f,limit:f32,current:RayHit)->RayHit {
    return dynamic_ray_cast_impl(origin,direction,limit,current,false);
}
fn dynamic_ray_cast_primary(origin:vec3f,direction:vec3f,limit:f32,current:RayHit)->RayHit {
    return dynamic_ray_cast_impl(origin,direction,limit,current,true);
}
fn dynamic_ray_flags(identity:u32)->u32 {
    let t=8u+(identity&0x7fffffffu)*DYN_TRIANGLE_WORDS;let m=dyn_geometry[t+32u];
    if dyn_assets[m]==3u {return ray_materials[dyn_assets[m+14u]].flags;}
    return 0u;
}
fn dynamic_ray_surface(hit:RayHit)->RaySurface {
    let index=hit.triangle&0x7fffffffu;let t=8u+index*DYN_TRIANGLE_WORDS;
    let m=dyn_geometry[t+32u];let instance=dyn_geometry[t+33u];let color=dyn_color(index,hit.uv).rgb;
    var pbr=bg_decode_pbr(vec4f(0.0),color,false,false);var emission=vec3f(0.0);
    var reflected=color;var transmitted=vec3f(0.0);var share=0.0;
    if dyn_assets[m]==3u {
        let id=dyn_assets[m+14u];let metadata=ray_materials[id];
        let texel=dyn_catalog_channels(hit.uv,i32(metadata.layer),(metadata.flags&16u)!=0u);
        pbr=bg_decode_pbr(texel,color,(metadata.flags&16u)!=0u,(metadata.flags&2u)!=0u);
        let glow=select(ray_emission[id],pbr.emission*max(1.0,ray_emission[id]),(metadata.flags&18u)==18u);
        emission=color*glow;
        if (metadata.flags&64u)!=0u {let optics=bg_foliage_optics(color,pbr.subsurface,metadata.flags);
            share=optics.share;reflected=optics.reflected;transmitted=optics.transmitted;}
    }
    return RaySurface(color,pbr,emission,dyn_ff(instance+31u),share,reflected,transmitted);
}
