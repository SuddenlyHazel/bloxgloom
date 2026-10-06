// Finite-sample path transport, with explicit emissive surfaces, visibility
// rays, diffuse/GGX sampling, Russian roulette, and participating media.
const RAY_PI:f32=3.14159265359;
struct RayFrame {
    inverse:mat4x4f,previous:mat4x4f,eye:vec4f,sun:vec4f,solar:vec4f,
    horizon:vec4f,zenith:vec4f,cloud:vec4f,climate:vec4f,parameters:vec4f,previous_eye:vec4f,counts:vec4u,
};
@group(0) @binding(0) var<uniform> ray_frame:RayFrame;
@group(0) @binding(3) var ray_depth:texture_depth_2d;
@group(0) @binding(4) var ray_receiver:texture_2d<f32>;
@group(0) @binding(5) var ray_response:texture_2d<f32>;
@group(0) @binding(6) var ray_indirect:texture_2d<f32>;
@group(0) @binding(7) var ray_history:texture_2d<f32>;
@group(0) @binding(8) var ray_history_geometry:texture_2d<f32>;
var<private> ray_rng:u32;
fn random()->f32 {
    ray_rng=ray_rng*747796405u+2891336453u;
    let word=((ray_rng>>((ray_rng>>28u)+4u))^ray_rng)*277803737u;
    return f32((word>>22u)^word)/4294967296.0;
}
fn ray_basis(n:vec3f)->mat3x3f {
    let axis=select(vec3f(0.0,1.0,0.0),vec3f(1.0,0.0,0.0),abs(n.y)>0.9);
    let tangent=normalize(cross(axis,n));return mat3x3f(tangent,cross(n,tangent),n);
}
fn cosine_ray(n:vec3f)->vec3f {
    let r=sqrt(random());let a=2.0*RAY_PI*random();
    return ray_basis(n)*vec3f(r*cos(a),r*sin(a),sqrt(max(0.0,1.0-r*r)));
}
fn environment(p:vec3f,d:vec3f,sky:f32)->vec3f {
    if sky<=0.0 {return vec3f(0.0);}
    return bg_sky_environment(p,d,ray_frame.sun,ray_frame.solar.xyz,ray_frame.cloud,ray_frame.climate.xy,ray_frame.horizon.xyz,ray_frame.zenith.xyz)*clamp(sky,0.0,1.0);
}
fn medium_density(p:vec3f)->f32 {
    return ray_frame.parameters.x*exp(-max(p.y-20.0,0.0)/90.0)+bg_cloud_density(p,ray_frame.cloud.x,ray_frame.cloud.yz);
}
// Delta tracking samples actual 3D cloud/air extinction. The .0854 majorant
// bounds both components. Scattering changes the path, not just its final fog.
fn medium_event(origin:vec3f,direction:vec3f,limit:f32)->f32 {
    var distance=0.0;
    let cloud=bg_cloud_interval(origin,direction,limit);
    if cloud.y<=cloud.x {
        let depth=ray_air_depth(origin.y,direction.y,limit);
        if depth<=0.0 {return limit;}
        // Optical distance is exponentially distributed even in nonuniform
        // air. Exact inversion replaces rejected majorant events, retaining
        // every real scattering event and its subsequent path continuation.
        let optical=ray_negative_log_one_minus(random());
        if optical>=depth {return limit;}
        return ray_air_distance(origin.y,direction.y,limit,optical);
    }
    // Air and the bounded cloud slab use separate majorants. Empty air must
    // not spend hundreds of rejected cloud-density samples per path.
    for(var i=0u;i<256u;i++) {
        let in_cloud=cloud.y>cloud.x&&distance>=cloud.x&&distance<cloud.y;
        var boundary=limit;
        if cloud.y>cloud.x {
            if distance<cloud.x {boundary=cloud.x;}
            else if in_cloud {boundary=cloud.y;}
        }
        let majorant=ray_frame.parameters.x+select(0.0,0.085,in_cloud);
        let next=distance-log(max(0.000001,1.0-random()))/max(majorant,0.00000001);
        if next>=boundary&&boundary<limit {distance=boundary+0.0001;continue;}
        distance=next;
        if distance>=limit {return limit;}
        if random()*majorant<medium_density(origin+direction*distance) {return distance;}
    }
    return limit;
}
// Air extinction is analytic; only the bounded cloud interval needs density
// quadrature. Keep the original32m nominal spacing (at most16 shadow samples),
// rather than spending the whole budget on a narrow slab or sampling empty air.
fn ray_segment_optical_depth(origin:vec3f,direction:vec3f,limit:f32)->f32 {
    var depth=ray_air_depth(origin.y,direction.y,limit);
    let cloud=bg_cloud_interval(origin,direction,limit);
    if cloud.y>cloud.x {
        let count=u32(clamp(ceil((cloud.y-cloud.x)/32.0),1.0,16.0));
        let step=(cloud.y-cloud.x)/f32(count);
        for(var i=0u;i<count;i++) {
            let point=origin+direction*(cloud.x+(f32(i)+0.5)*step);
            depth+=bg_cloud_density(point,ray_frame.cloud.x,ray_frame.cloud.yz)*step;
        }
    }
    return depth;
}
fn medium_anisotropy(p:vec3f)->f32 {
    return select(0.0,0.60,bg_cloud_density(p,ray_frame.cloud.x,ray_frame.cloud.yz)>ray_frame.parameters.x);
}
fn medium_phase(cosine:f32,g:f32)->f32 {
    return (1.0-g*g)/(4.0*RAY_PI*pow(max(1.0+g*g-2.0*g*cosine,0.0001),1.5));
}
fn medium_direction(incoming:vec3f,g:f32)->vec3f {
    let xi=random();var cosine=xi*2.0-1.0;
    if abs(g)>0.001 {let ratio=(1.0-g*g)/(1.0-g+2.0*g*xi);cosine=(1.0+g*g-ratio*ratio)/(2.0*g);}
    let sine=sqrt(max(0.0,1.0-cosine*cosine));let a=2.0*RAY_PI*random();
    return ray_basis(incoming)*vec3f(sine*cos(a),sine*sin(a),cosine);
}
fn sun_light_visible(p:vec3f,sky:f32)->vec3f {
    if max(max(ray_frame.solar.x,ray_frame.solar.y),ray_frame.solar.z)<=0.0 {return vec3f(0.0);}
    if sky<=0.0&&!ray_certified_sky(p,normalize(ray_frame.sun.xyz)) {return vec3f(0.0);}
    let direction=normalize(ray_frame.sun.xyz);
    // Optional measured optimization. No opaque blocker means the exact
    // original ordered botanical transmission path still runs below.
    if ray_frame.parameters.w>0.5 && ray_any_opaque(p,direction,512.0) {return vec3f(0.0);}
    var origin=p;var transmission=vec3f(1.0);var clear=false;
    for(var i=0u;i<8u;i++) {
        let hit=ray_cast(origin,direction,512.0);
        if hit.triangle==0xffffffffu {clear=true;break;}
        if RAY_MATERIAL_FAST && (ray_materials[u32(ray_triangles[hit.triangle].a.w)].flags&64u)==0u {return vec3f(0.0);}
        let sheet=ray_surface(hit);
        if sheet.transmission<=0.0 {return vec3f(0.0);}
        transmission*=sheet.transmittance*(vec3f(1.0)-sheet.pbr.f0);
        origin+=direction*(hit.distance+0.006);
    }
    if !clear {return vec3f(0.0);}
    // Extinction includes air and clouds on the same bounded shadow segment.
    let optical_depth=ray_segment_optical_depth(p,direction,512.0);
    return transmission*ray_frame.solar.xyz*exp(-optical_depth);
}
fn sun_light(p:vec3f)->vec3f {return sun_light_visible(p,1.0);}
struct RaySurface {albedo:vec3f,pbr:BgPbr,emission:vec3f,sky:f32,transmission:f32,reflection:vec3f,transmittance:vec3f};
fn ray_surface(hit:RayHit)->RaySurface {
    let triangle=ray_triangles[hit.triangle];let id=u32(triangle.a.w);let metadata=ray_materials[id];
    let color=textureSampleLevel(ray_albedo,ray_sampler,hit.uv,i32(metadata.layer),1.0).rgb;
    let filtered=textureSampleLevel(ray_specular,ray_sampler,hit.uv,i32(metadata.layer),1.0);
    var texel=filtered;
    if !RAY_MATERIAL_FAST||(metadata.flags&16u)!=0u {
        let size=textureDimensions(ray_specular,0);
        let pixel=min(vec2i(floor(fract(hit.uv)*vec2f(size))),vec2i(size)-vec2i(1));
        let categorical=textureLoad(ray_specular,pixel,i32(metadata.layer),0);
        texel=select(filtered,vec4f(filtered.r,categorical.g,categorical.b,filtered.a),(metadata.flags&16u)!=0u);
    }
    let pbr=bg_decode_pbr(texel,color,(metadata.flags&16u)!=0u,(metadata.flags&2u)!=0u);
    let glow=select(ray_emission[id],pbr.emission*max(1.0,ray_emission[id]),(metadata.flags&18u)==18u);
    let botanical=(metadata.flags&64u)!=0u;
    if RAY_MATERIAL_FAST&&!botanical {return RaySurface(color,pbr,color*glow,triangle.b.w,0.0,color,vec3f(0.0));}
    let optics=bg_foliage_optics(color,pbr.subsurface,metadata.flags);
    let thin=select(0.0,optics.share,botanical);
    return RaySurface(color,pbr,color*glow,triangle.b.w,thin,
        select(color,optics.reflected,botanical),select(vec3f(0.0),optics.transmitted,botanical));
}
struct RayScatter {direction:vec3f,weight:vec3f};
fn scatter(n:vec3f,v:vec3f,surface:RaySurface)->RayScatter {
    let probability=select(0.25,1.0,surface.pbr.metal>0.5);
    if random()>=probability {
        let f=bg_pbr_fresnel(max(dot(n,v),0.0),surface.pbr);
        // Split a thin botanical sheet's non-specular energy between reflected
        // and transmitted hemispheres. Sampling probability cancels each lobe's
        // strength, preserving energy instead of adding fake unoccluded fill.
        let transmitted=random()<surface.transmission;
        let allocation=select(surface.reflection/max(1.0-surface.transmission,0.0001),
            surface.transmittance/max(surface.transmission,0.0001),transmitted);
        return RayScatter(cosine_ray(select(n,-n,transmitted)),allocation*(1.0-surface.pbr.metal)*(vec3f(1.0)-f)/(1.0-probability));
    }
    let alpha=surface.pbr.roughness*surface.pbr.roughness;
    let xi=random();let azimuth=2.0*RAY_PI*random();
    let cosine=sqrt((1.0-xi)/(1.0+(alpha*alpha-1.0)*xi));let sine=sqrt(max(0.0,1.0-cosine*cosine));
    let h=ray_basis(n)*vec3f(sine*cos(azimuth),sine*sin(azimuth),cosine);
    let d=reflect(-v,h);let nl=max(dot(n,d),0.0);let nv=max(dot(n,v),0.0001);let vh=max(dot(v,h),0.0);
    let k=(surface.pbr.roughness+1.0)*(surface.pbr.roughness+1.0)/8.0;
    var geometry=nl/max(nl*(1.0-k)+k,0.0001)*nv/max(nv*(1.0-k)+k,0.0001);
    if surface.pbr.preset_id!=0u {geometry=1.0/(1.0+bg_ggx_lambda(nl,alpha)+bg_ggx_lambda(nv,alpha));}
    let fresnel=bg_pbr_fresnel(vh,surface.pbr);
    let weight=fresnel*geometry*vh/max(nv*cosine,0.0001)/probability;
    return RayScatter(d,select(vec3f(0.0),weight,nl>0.0));
}
// Directional irradiance evaluated with the reflected/transmitted Lambert BRDF.
fn ray_diffuse_direct(n:vec3f,v:vec3f,sun:vec3f,surface:RaySurface,solar:vec3f)->vec3f {
    let cosine=dot(n,sun);
    let fresnel=bg_pbr_fresnel(max(dot(n,v),0.0),surface.pbr);
    return (surface.reflection*max(cosine,0.0)+surface.transmittance*max(-cosine,0.0))
        *(1.0-surface.pbr.metal)*(vec3f(1.0)-fresnel)*solar/RAY_PI;
}
fn transport(start:vec3f,initial:vec3f,initial_sky:f32)->vec3f {
    var origin=start;var direction=initial;var throughput=vec3f(1.0);var radiance=vec3f(0.0);
    var sky_visibility=initial_sky;
    for(var bounce=0u;bounce<12u;bounce++) {
        let hit=ray_cast(origin,direction,512.0);
        var sky_exit=-1.0;
        if hit.triangle==0xffffffffu && sky_visibility<1.0 {sky_exit=ray_sky_exit_distance(origin,direction);}
        let event=medium_event(origin,direction,hit.distance);
        // Retain a witnessed exterior crossing when the next event scatters
        // beyond coverage. An indoor scatter before the boundary proves nothing.
        if sky_exit>=0.0 && event>=sky_exit {sky_visibility=1.0;}
        if event<hit.distance {
            origin+=direction*event;
            let g=medium_anisotropy(origin);
            radiance+=throughput*sun_light(origin)*medium_phase(dot(direction,ray_frame.sun.xyz),g)*0.92;
            throughput*=0.92;
            direction=medium_direction(direction,g);
        } else {
            if hit.triangle==0xffffffffu {
                radiance+=throughput*environment(origin+direction*hit.distance,direction,sky_visibility);break;
            }
            origin+=direction*hit.distance;
            let surface=ray_surface(hit);
            let sunlight=sun_light_visible(origin+hit.normal*select(-0.006,0.006,dot(hit.normal,ray_frame.sun.xyz)>=0.0),surface.sky);
            let diffuse=ray_diffuse_direct(hit.normal,-direction,ray_frame.sun.xyz,surface,sunlight);
            let specular=bg_pbr_sun(hit.normal,-direction,ray_frame.sun,1.0,1.0,surface.pbr,sunlight);
            radiance+=throughput*(surface.emission+diffuse+specular);
            let sample=scatter(hit.normal,-direction,surface);
            throughput*=sample.weight;direction=sample.direction;origin+=hit.normal*select(-0.006,0.006,dot(direction,hit.normal)>=0.0);
        }
        // Exact zero throughput cannot contribute at later vertices. Keep
        // this vertex's already-accumulated direct/emissive radiance intact.
        if all(throughput==vec3f(0.0)) {break;}
        if bounce>=2u {
            let survive=clamp(max(max(throughput.x,throughput.y),throughput.z),0.05,0.95);
            if random()>survive {break;}
            throughput/=survive;
        }
    }
    return max(radiance,vec3f(0.0));
}
struct Fullscreen { @builtin(position) position:vec4f };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Fullscreen {
    let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return Fullscreen(vec4f(xy*2.0-1.0,0.0,1.0));
}
struct RayTransportOutput { @location(0) radiance:vec4f, @location(1) geometry:vec4f, @location(2) transmission:f32 };
fn ray_primary_transport(frag:vec4f)->RayTransportOutput {
    let size=vec2i(textureDimensions(ray_depth));
    let stride=max(2,i32(ray_frame.parameters.z));
    let pixel=min(vec2i(frag.xy)*stride+vec2i(stride/2),size-vec2i(1));
    var receiver=textureLoad(ray_receiver,pixel,0);
    let media_only=receiver.z<=0.0||receiver.w<=0.0;
    let ndc=(vec2f(pixel)+0.5)/vec2f(size)*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let far=ray_frame.inverse*vec4f(ndc,1.0,1.0);let direction=normalize(far.xyz/far.w-ray_frame.eye.xyz);
    if media_only {
        let depth=textureLoad(ray_depth,pixel,0);
        var distance=2400.0;
        if depth<0.999999 {
            let point=ray_frame.inverse*vec4f(ndc,depth,1.0);
            distance=length(point.xyz/point.w-ray_frame.eye.xyz);
        }
        receiver=vec4f(oct_encode(-direction),2.0,distance);
    }
    let depth_gradient=max(abs(dpdx(receiver.w)),abs(dpdy(receiver.w)));
    let position=ray_frame.eye.xyz+direction*receiver.w;
    let depth_normal=cross(dpdx(position),dpdy(position));
    var n=oct_decode(receiver.xy);
    if dot(n,-direction)<0.0 {n=-n;}
    ray_rng=u32(pixel.x)*1973u+u32(pixel.y)*9277u+ray_frame.counts.z*26699u+911u;
    var opaque=RayHit(receiver.w,0xffffffffu,vec2f(0.0),vec3f(0.0));
    let raster_indirect=textureLoad(ray_indirect,pixel,0);
    let water=!media_only&&ray_is_water(raster_indirect.a);
    var valid_opaque=false;
    if !media_only&&!water {
        opaque=ray_cast(ray_frame.eye.xyz,direction,receiver.w+0.1);
        valid_opaque=opaque.triangle!=0xffffffffu&&abs(opaque.distance-receiver.w)<=max(0.10,receiver.w*0.002);
    }
    let replace_surface=media_only||water||valid_opaque;
    let response=textureLoad(ray_response,pixel,0);
    var delta=vec3f(0.0);
    if media_only {
        // Sky and non-PBR geometry still participate in camera-segment media.
        // Their correction uses a unit basis; no borrowed opaque albedo applies.
    } else if water {
        let ray=reflect(direction,n);
        let reflected=transport(position+n*0.01,ray,response.w);
        delta=reflected*response.rgb;
    } else if valid_opaque {
        var surface=ray_surface(opaque);
        surface.pbr.roughness=receiver.z;
        let sample=scatter(n,-direction,surface);
        var indirect=vec3f(0.0);
        // Rejected GGX directions have exactly zero sample weight. Their
        // transport would consume work/RNG but cannot change expected energy.
        if any(sample.weight!=vec3f(0.0)) {
            indirect=sample.weight*transport(position+opaque.normal*select(-0.01,0.01,dot(sample.direction,opaque.normal)>=0.0),sample.direction,surface.sky);
        }
        let old_diffuse=textureLoad(ray_indirect,pixel,0);
        // Deterministic sky-specular removal belongs to the full-resolution
        // center texel, after filtering; it must never smear across normal maps.
        delta=indirect-old_diffuse.rgb*abs(old_diffuse.a);
    }
    let medium=ray_primary_medium_sample(ray_frame.eye.xyz,direction,receiver.w,textureLoad(ray_scene,pixel,0).rgb,delta);
    delta=medium.correction;
    let filtering_basis=select(ray_filter_basis(raster_indirect),vec3f(1.0),water||media_only);
    delta/=filtering_basis;
    // Metadata uses geometric normals: high-frequency normal-map texels must
    // not continually reset diffuse illumination history on an otherwise flat face.
    var geometric_normal=n;
    if valid_opaque {geometric_normal=opaque.normal;}
    else if !water&&!media_only&&dot(depth_normal,depth_normal)>0.00000001 {
        geometric_normal=normalize(depth_normal);
        if dot(geometric_normal,-direction)<0.0 {geometric_normal=-geometric_normal;}
    }
    var max_age=select(32.0,6.0,water||media_only);
    if media_only {max_age=32.0;}
    if valid_opaque {
        let metadata=ray_materials[u32(ray_triangles[opaque.triangle].a.w)];
        if (metadata.flags&64u)!=0u {max_age=16.0;}
    }
    var age=1.0;
    let previous=ray_frame.previous*vec4f(position,1.0);
    let previous_uv=previous.xy/previous.w*vec2f(0.5,-0.5)+0.5;
    let history_size=vec2i(textureDimensions(ray_history));
    if replace_surface && ray_frame.counts.w!=0u && previous.w>0.0 && all(previous_uv>vec2f(0.0)) && all(previous_uv<vec2f(1.0)) {
        let hp=clamp(vec2i(previous_uv*vec2f(history_size)),vec2i(0),history_size-vec2i(1));
        let old=textureLoad(ray_history,hp,0);
        let old_geometry=textureLoad(ray_history_geometry,hp,0);
        // Alpha records radial eye distance, not clip W/view-axis depth. Include
        // the half-resolution pixel footprint on a sloping surface, not just f16
        // depth quantization. Scene revisions/camera cuts disable history on CPU.
        let expected_depth=length(position-ray_frame.previous_eye.xyz);
        if ray_history_compatible(old.a,expected_depth,old_geometry,geometric_normal,receiver.z,water,depth_gradient) {
            age=min(old_geometry.w+1.0,max_age);
            delta=mix(delta,old.rgb,1.0-1.0/age);
        }
    }
    return RayTransportOutput(vec4f(delta,receiver.w),
        // An unmatched raster surface keeps its ambient/specular fallback.
        // Negative age rejects history when a true hit becomes available; the
        // signed primary T prevents composite from removing retained specular.
        vec4f(oct_encode(geometric_normal),select(receiver.z,-receiver.z,water),select(-1.0,age,replace_surface)),
        select(-medium.transmission,medium.transmission,replace_surface));
}

@fragment fn fs_transport(@builtin(position) frag:vec4f)->RayTransportOutput {
    return ray_primary_transport(frag);
}
