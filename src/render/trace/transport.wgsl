// Finite-sample path transport, with explicit emissive surfaces, visibility
// rays, diffuse/GGX sampling, Russian roulette, and participating media.
const RAY_PI:f32=3.14159265359;
struct RayFrame {
    inverse:mat4x4f,previous:mat4x4f,eye:vec4f,sun:vec4f,solar:vec4f,
    horizon:vec4f,zenith:vec4f,cloud:vec4f,climate:vec4f,parameters:vec4f,previous_eye:vec4f,counts:vec4u,water:vec4f,
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
// Straight next-event rays terminate at dielectric interfaces. Refractive
// sunlight is evaluated at actual water vertices, including their BTDF, rather
// than pretending the unrefracted air direction passes through colored glass.
fn ray_sun_visibility(p:vec3f,direction:vec3f,sky:f32)->vec3f {
    if sky<=0.0&&!ray_certified_sky(p,direction) {return vec3f(0.0);}
    if ray_water_at(p)==1 {return vec3f(0.0);}
    if ray_frame.parameters.w>0.5 && ray_any_opaque(p,direction,512.0) {return vec3f(0.0);}
    var origin=p;var transmission=vec3f(1.0);var clear=false;
    for(var i=0u;i<8u;i++) {
        let hit=ray_cast(origin,direction,512.0);
        if hit.triangle==0xffffffffu {clear=true;break;}
        if ray_water_is(hit) {return vec3f(0.0);}
        let sheet=ray_surface(hit,origin+direction*hit.distance);
        if sheet.transmission<=0.0 {return vec3f(0.0);}
        transmission*=sheet.transmittance*(vec3f(1.0)-sheet.pbr.f0);
        origin+=direction*(hit.distance+0.006);
    }
    if !clear {return vec3f(0.0);}
    return transmission*exp(-ray_segment_optical_depth(p,direction,512.0));
}
fn sun_light_visible(p:vec3f,sky:f32)->vec3f {
    if max(max(ray_frame.solar.x,ray_frame.solar.y),ray_frame.solar.z)<=0.0 {return vec3f(0.0);}
    return ray_frame.solar.xyz*ray_sun_visibility(p,normalize(ray_frame.sun.xyz),sky);
}
fn sun_light(p:vec3f)->vec3f {return sun_light_visible(p,1.0);}
fn ray_water_direct(hit:RayHit,position:vec3f,incoming:vec3f,boundary:RayWaterInterface,split:bool)->vec3f {
    if all(ray_frame.solar.xyz<=vec3f(0.0)) {return vec3f(0.0);}
    let light=ray_water_sun_direction(ray_frame.sun.xyz,vec2f(random(),random()));
    let evaluated=ray_water_eval(boundary,incoming,light);
    if evaluated.pdf<=0.0 {return vec3f(0.0);}
    let origin=position+boundary.outward*select(-0.006,0.006,dot(light,boundary.outward)>=0.0);
    // A connector ending inside water needs another actual refractive vertex;
    // normal path continuation samples it, retaining arbitrary interface chains.
    if ray_water_at(origin)==1 {return vec3f(0.0);}
    let visibility=ray_sun_visibility(origin,light,ray_triangle_at(hit.triangle).b.w);
    // This direction was generated by the cone sampler: its PDF/support is
    // known analytically. Re-testing a rounded hard-edge dot product could
    // turn the same sample into0/0 near the solar disc's float32 boundary.
    let pdf=ray_water_sun_cone_pdf();
    let emission=ray_frame.solar.xyz/(RAY_PI*sin(RAY_WATER_SUN_RADIUS)*sin(RAY_WATER_SUN_RADIUS));
    return evaluated.f*abs(dot(boundary.normal,light))*visibility*emission/pdf
        *ray_water_power_weight(pdf,select(evaluated.pdf,ray_water_conditional_pdf(boundary,incoming,light),split));
}
struct RaySurface {albedo:vec3f,pbr:BgPbr,emission:vec3f,sky:f32,transmission:f32,reflection:vec3f,transmittance:vec3f};
fn ray_surface(hit:RayHit,position:vec3f)->RaySurface {
    if (hit.triangle&0x80000000u)!=0u {return dynamic_ray_surface(hit);}
    let triangle=ray_triangle_at(hit.triangle);let id=u32(triangle.a.w);let metadata=ray_materials[id];
    if (triangle.surface_flags&5u)==5u&&(triangle.surface_flags&8u)==0u {
        let color=unpack4x8unorm(triangle.surface_color).rgb;
        let pbr=bg_decode_pbr(vec4f(0.0),color,false,false);
        let glow=f32((triangle.surface_flags>>8u)&15u)/15.0;
        return RaySurface(color,pbr,color*glow*glow*vec3f(1.0,0.57,0.23),triangle.b.w,0.0,color,vec3f(0.0));
    }
    var color=textureSampleLevel(ray_albedo,ray_sampler,hit.uv,i32(metadata.layer),1.0).rgb;
    let coarse=(triangle.surface_flags&4u)!=0u;
    if coarse {
        let baked=unpack4x8unorm(triangle.surface_color).rgb;
        let blend=select(0.0,1.0-smoothstep(96.0,240.0,distance(position,ray_frame.eye.xyz)),(triangle.surface_flags&8u)!=0u);
        color=mix(baked,color,blend);
    }
    let filtered=textureSampleLevel(ray_specular,ray_sampler,hit.uv,i32(metadata.layer),1.0);
    var texel=filtered;
    if !RAY_MATERIAL_FAST||(metadata.flags&16u)!=0u {
        let size=textureDimensions(ray_specular,0);
        let pixel=min(vec2i(floor(fract(hit.uv)*vec2f(size))),vec2i(size)-vec2i(1));
        let categorical=textureLoad(ray_specular,pixel,i32(metadata.layer),0);
        texel=select(filtered,vec4f(filtered.r,categorical.g,categorical.b,filtered.a),(metadata.flags&16u)!=0u);
    }
    var pbr=bg_decode_pbr(texel,color,(metadata.flags&16u)!=0u,(metadata.flags&2u)!=0u);
    if coarse&&(triangle.surface_flags&8u)==0u {pbr=bg_decode_pbr(vec4f(0.0),color,false,false);}
    var glow=select(ray_emission[id],pbr.emission*max(1.0,ray_emission[id]),(metadata.flags&18u)==18u);
    var emission=color*glow;
    if (triangle.surface_flags&1u)!=0u {
        glow=f32((triangle.surface_flags>>8u)&15u)/15.0;
        emission=color*glow*glow*vec3f(1.0,0.57,0.23);
    }
    let botanical=(metadata.flags&64u)!=0u;
    if RAY_MATERIAL_FAST&&!botanical {return RaySurface(color,pbr,emission,triangle.b.w,0.0,color,vec3f(0.0));}
    let optics=bg_foliage_optics(color,pbr.subsurface,metadata.flags);
    let thin=select(0.0,optics.share,botanical);
    return RaySurface(color,pbr,emission,triangle.b.w,thin,
        select(color,optics.reflected,botanical),select(vec3f(0.0),optics.transmitted,botanical));
}
struct RayScatter {direction:vec3f,weight:vec3f};
fn scatter(n:vec3f,v:vec3f,surface:RaySurface,position:vec3f)->RayScatter {
    let probability=select(0.0,select(0.25,1.0,surface.pbr.metal>0.5),surface.pbr.present);
    if random()>=probability {
        let f=select(vec3f(0.0),bg_pbr_fresnel(max(dot(n,v),0.0),surface.pbr),surface.pbr.present);
        // Split a thin botanical sheet's non-specular energy between reflected
        // and transmitted hemispheres. Sampling probability cancels each lobe's
        // strength, preserving energy instead of adding fake unoccluded fill.
        let transmitted=random()<surface.transmission;
        let allocation=select(surface.reflection/max(1.0-surface.transmission,0.0001),
            surface.transmittance/max(surface.transmission,0.0001),transmitted);
        let normal=select(n,-n,transmitted);
        var guide=RayWaterGuide(normal,1.0,false);
        if !transmitted&&surface.transmission==0.0 {guide=ray_water_diffuse_guide(position,normal);}
        let sampled=ray_water_guide_sample(normal,guide,vec3f(random(),random(),random()));
        return RayScatter(sampled.direction,allocation*(1.0-surface.pbr.metal)*(vec3f(1.0)-f)*sampled.weight/(1.0-probability));
    }
    var guide=RayWaterGuide(n,1.0,false);
    if RAY_CAUSTIC_GGX_GUIDE&&surface.transmission==0.0 {guide=ray_water_diffuse_guide(position,n);}
    let sample=ray_water_guided_ggx(n,v,surface.pbr,guide,vec3f(random(),random(),random()));
    return RayScatter(sample.direction,sample.weight/probability);
}
// Directional irradiance evaluated with the reflected/transmitted Lambert BRDF.
fn ray_diffuse_direct(n:vec3f,v:vec3f,sun:vec3f,surface:RaySurface,solar:vec3f)->vec3f {
    let cosine=dot(n,sun);
    let fresnel=select(vec3f(0.0),bg_pbr_fresnel(max(dot(n,v),0.0),surface.pbr),surface.pbr.present);
    return (surface.reflection*max(cosine,0.0)+surface.transmittance*max(-cosine,0.0))
        *(1.0-surface.pbr.metal)*(vec3f(1.0)-fresnel)*solar/RAY_PI;
}
// Keep the actual sunlight query and its receiver offset in one place. The
// skipped branch has identically zero reflected and transmitted solar energy.
fn ray_surface_sunlight(origin:vec3f,normal:vec3f,surface:RaySurface,enabled:bool)->vec3f {
    if enabled && dot(normal,ray_frame.sun.xyz)<=0.0 && all(surface.transmittance==vec3f(0.0)) {
        return vec3f(0.0);
    }
    return sun_light_visible(origin+normal*select(-0.006,0.006,dot(normal,ray_frame.sun.xyz)>=0.0),surface.sky);
}
fn transport_state(start:vec3f,initial:vec3f,initial_sky:f32,first_limit:f32,first_primary:bool,first_footprint:vec4f,initial_depth:u32,initial_pdf:f32,initial_eta:f32)->vec3f {
    var origin=start;var direction=initial;var throughput=vec3f(1.0);var radiance=vec3f(0.0);
    var sky_visibility=initial_sky;var eta_scale=initial_eta;var last_water_pdf=initial_pdf;
    var water_medium=ray_water_at(origin)==1;var water_scattered=false;
    for(var bounce=0u;bounce<12u;bounce++) {
        if bounce+initial_depth>=12u {break;}
        var hit=RayHit(first_limit,0xffffffffu,vec2f(0.0),vec3f(0.0));
        if first_primary&&bounce==0u {hit=ray_cast_primary(origin,direction,first_limit);}
        else {hit=ray_cast(origin,direction,select(512.0,first_limit,bounce==0u));}
        var sky_exit=-1.0;
        if !water_medium&&hit.triangle==0xffffffffu&&sky_visibility<1.0 {
            sky_exit=ray_sky_exit_distance(origin,direction);
        }
        var limit=hit.distance;
        if water_medium {limit=min(limit,ray_water_known_distance(origin,direction,limit));}
        var event=limit;
        if water_medium {
            let medium=ray_water_medium_sample(limit,vec2f(random(),random()));
            throughput*=medium.weight;event=medium.distance;
        } else {event=medium_event(origin,direction,limit);}
        if sky_exit>=0.0&&event>=sky_exit {sky_visibility=1.0;}
        if event<limit {
            origin+=direction*event;
            if water_medium {
                water_scattered=true;
                // Spectral absorption/scattering is already in the mixture
                // weight. Subsequent real events continue, with no air albedo.
                var guide=RayWaterGuide(direction,1.0,false);
                if RAY_CAUSTIC_PHASE_GUIDE {guide=ray_water_sun_guide(origin,direction);}
                let phase=ray_water_guided_phase(direction,guide,vec3f(random(),random(),random()));
                throughput*=phase.weight;direction=phase.direction;
            } else {
                let g=medium_anisotropy(origin);
                radiance+=ray_water_component(throughput*sun_light(origin)*medium_phase(dot(direction,ray_frame.sun.xyz),g)*0.92,water_scattered);
                throughput*=0.92;direction=medium_direction(direction,g);
            }
            last_water_pdf=0.0;
        } else {
            // Missing residency/coverage is not a dry-water exit or proof of
            // exterior sky. Do not extend extinction into unseen voxel state.
            if water_medium&&limit<hit.distance-0.0001 {break;}
            if hit.triangle==0xffffffffu {
                if !water_medium {
                    var sky=environment(origin+direction*hit.distance,direction,sky_visibility);
                    if last_water_pdf>0.0&&sky_visibility>0.0 {
                        let sun_pdf=ray_water_sun_pdf(ray_frame.sun.xyz,direction);
                        sky+=ray_water_sun_emission(ray_frame.sun.xyz,ray_frame.solar.xyz,direction)
                            *ray_water_power_weight(last_water_pdf,sun_pdf)*sky_visibility;
                    }
                    radiance+=ray_water_component(throughput*sky,water_scattered);
                }
                break;
            }
            origin+=direction*hit.distance;
            if ray_water_is(hit) {
                let boundary=ray_water_interface(hit,direction,origin,select(vec4f(0.0),first_footprint,first_primary&&bounce==0u));
                radiance+=ray_water_component(throughput*ray_water_direct(hit,origin,direction,boundary,false),water_scattered);
                let sample=ray_water_sample(boundary,direction,vec3f(random(),random(),random()));
                throughput*=sample.weight;
                eta_scale*=sample.eta*sample.eta;
                last_water_pdf=sample.pdf;direction=sample.direction;
                origin+=boundary.outward*select(-0.006,0.006,dot(direction,boundary.outward)>=0.0);
                let state=ray_water_at(origin);
                if state<0&&sample.transmitted {break;}
                if state>=0 {water_medium=state==1;}
                if !water_medium {sky_visibility=max(sky_visibility,ray_triangle_at(hit.triangle).b.w);}
            } else {
                let surface=ray_surface(hit,origin);
                let sunlight=ray_surface_sunlight(origin,hit.normal,surface,RAY_SUN_SKIP);
                let diffuse=ray_diffuse_direct(hit.normal,-direction,ray_frame.sun.xyz,surface,sunlight);
                let specular=bg_pbr_sun(hit.normal,-direction,ray_frame.sun,1.0,1.0,surface.pbr,sunlight);
                radiance+=ray_water_component(throughput*(surface.emission+diffuse+specular),water_scattered);
                let sample=scatter(hit.normal,-direction,surface,origin);
                throughput*=sample.weight;direction=sample.direction;
                origin+=hit.normal*select(-0.006,0.006,dot(direction,hit.normal)>=0.0);
                // Reflection at an opaque underwater bottom remains submerged.
                water_medium=ray_water_at(origin)==1;last_water_pdf=0.0;
            }
        }
        if all(throughput==vec3f(0.0)) {break;}
        if bounce+initial_depth>=2u {
            // Refractive radiance scaling must not prematurely kill a path that
            // recovers the reciprocal eta factor when it exits the same volume.
            let survive=clamp(max(max(throughput.x,throughput.y),throughput.z)*eta_scale,0.05,0.95);
            if random()>survive {break;}
            throughput/=survive;
        }
    }
    return max(radiance,vec3f(0.0));
}
fn transport_limit(start:vec3f,initial:vec3f,initial_sky:f32,first_limit:f32,first_primary:bool,first_footprint:vec4f)->vec3f {
    return transport_state(start,initial,initial_sky,first_limit,first_primary,first_footprint,0u,0.0,1.0);
}
fn ray_primary_water_radiance(hit:RayHit,position:vec3f,incoming:vec3f,footprint:vec4f)->vec3f {
    let boundary=ray_water_interface(hit,incoming,position,footprint);
    var result=ray_water_component(ray_water_direct(hit,position,incoming,boundary,true),false);
    for(var branch=0u;branch<2u;branch++) {
        let sample=ray_water_conditional_sample(boundary,incoming,vec2f(random(),random()),branch==1u);
        if all(sample.weight==vec3f(0.0)) {continue;}
        let origin=position+boundary.outward*select(-0.006,0.006,dot(sample.direction,boundary.outward)>=0.0);
        let state=ray_water_at(origin);
        if state<0&&sample.transmitted {continue;}
        let sky=ray_triangle_at(hit.triangle).b.w;
        result+=sample.weight*transport_state(origin,sample.direction,sky,512.0,false,vec4f(0.0),1u,sample.pdf,sample.eta*sample.eta);
    }
    return result;
}
fn transport(start:vec3f,initial:vec3f,initial_sky:f32)->vec3f {
    return transport_limit(start,initial,initial_sky,512.0,false,vec4f(0.0));
}
struct Fullscreen { @builtin(position) position:vec4f };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Fullscreen {
    let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return Fullscreen(vec4f(xy*2.0-1.0,0.0,1.0));
}
struct RayTransportOutput { @location(0) radiance:vec4f, @location(1) geometry:vec4f, @location(2) transmission:vec4f, @location(3) current:vec4f };
fn ray_primary_transport(frag:vec4f)->RayTransportOutput {
    let size=vec2i(textureDimensions(ray_depth));
    let stride=max(2,i32(ray_frame.parameters.z));
    let pixel=min(vec2i(frag.xy)*stride+vec2i(stride/2),size-vec2i(1));
    var receiver=textureLoad(ray_receiver,pixel,0);
    var media_only=receiver.z<=0.0||receiver.w<=0.0;
    let ndc=(vec2f(pixel)+0.5)/vec2f(size)*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let far=ray_frame.inverse*vec4f(ndc,1.0,1.0);
    let direction=normalize(far.xyz/far.w);
    let depth=textureLoad(ray_depth,pixel,0);
    var opaque_distance=2400.0;
    if depth<0.999999 {
        let point=ray_frame.inverse*vec4f(ndc,depth,1.0);
        opaque_distance=length(point.xyz/point.w);
    }
    if media_only {receiver=vec4f(oct_encode(-direction),2.0,opaque_distance);}
    let raster_indirect=textureLoad(ray_indirect,pixel,0);
    let raster_water=!media_only&&ray_is_water(raster_indirect.a);
    let response=textureLoad(ray_response,pixel,0);
    ray_rng=u32(pixel.x)*1973u+u32(pixel.y)*9277u+ray_frame.counts.z*26699u+911u;
    // Original depth is opaque-only. Select the nearest real interface, not
    // the last water layer's alpha-composited MRT or chunk-distance sort order.
    var first=RayHit(receiver.w,0xffffffffu,vec2f(0.0),vec3f(0.0));
    if !media_only||ray_frame.water.z>0.5 {
        first=ray_cast_primary(ray_frame.eye.xyz,direction,opaque_distance+0.1);
    }
    let first_water=ray_water_is(first);
    let first_position=ray_frame.eye.xyz+direction*first.distance;
    let first_footprint=vec4f(dpdx(first_position.xz),dpdy(first_position.xz))/f32(stride);
    var classified_interface=false;
    if first_water {
        classified_interface=ray_water_at(first_position-direction*0.006)>=0
            &&ray_water_at(first_position+direction*0.006)>=0;
    }
    let eye_water=ray_frame.water.z>0.5&&ray_water_at(ray_frame.eye.xyz)==1;
    let complete_path=classified_interface||(eye_water&&first.triangle!=0xffffffffu);
    var n=oct_decode(receiver.xy);
    if dot(n,-direction)<0.0 {n=-n;}
    if complete_path {
        receiver.w=first.distance;
        if first_water {
            let boundary=ray_water_interface(first,direction,first_position,first_footprint);
            receiver.z=boundary.roughness;n=boundary.normal;
        } else if media_only {receiver.z=ray_surface(first,first_position).pbr.roughness;}
        media_only=false;
    }
    let depth_gradient=max(abs(dpdx(receiver.w)),abs(dpdy(receiver.w)));
    let position=ray_frame.eye.xyz+direction*receiver.w;
    let depth_normal=cross(dpdx(position),dpdy(position));
    let valid_opaque=!media_only&&!raster_water&&!first_water&&first.triangle!=0xffffffffu
        &&abs(first.distance-receiver.w)<=max(0.10,receiver.w*0.002);
    let replace_surface=complete_path||media_only||valid_opaque;
    let primary_actor=first.triangle!=0xffffffffu&&(first.triangle&0x80000000u)!=0u;
    let initial_rng=ray_rng;
    ray_dynamic_disabled=false;ray_dynamic_touched=false;
    let full=ray_lighting_sample(complete_path,first_water,eye_water,valid_opaque,
        raster_water,media_only,first,first_position,first_footprint,direction,
        opaque_distance,response,receiver,n,position,raster_indirect);
    let complete_rng=ray_rng;
    var delta=full.radiance;var reflection=full.reflection;var current=vec3f(0.0);
    if ray_dynamic_touched&&!primary_actor {
        ray_rng=initial_rng;ray_dynamic_disabled=true;
        let static_sample=ray_lighting_sample(complete_path,first_water,eye_water,valid_opaque,
            raster_water,media_only,first,first_position,first_footprint,direction,
            opaque_distance,response,receiver,n,position,raster_indirect);
        delta=static_sample.radiance;reflection=static_sample.reflection;current=full.radiance-static_sample.radiance;
        ray_dynamic_disabled=false;ray_rng=complete_rng;
    }
    let primary_t=full.transmission;
    var geometric_normal=n;
    if complete_path||valid_opaque {geometric_normal=first.normal;}
    else if !raster_water&&!media_only&&dot(depth_normal,depth_normal)>0.00000001 {
        geometric_normal=normalize(depth_normal);
        if dot(geometric_normal,-direction)<0.0 {geometric_normal=-geometric_normal;}
    }
    var geometry_class=select(receiver.z,-receiver.z,raster_water);
    if complete_path {geometry_class=-(3.0+receiver.z);}
    var max_age=select(32.0,6.0,raster_water&&!complete_path);
    if valid_opaque&&!complete_path {
        if (first.triangle&0x80000000u)!=0u {
            if (dynamic_ray_flags(first.triangle)&64u)!=0u {max_age=16.0;}
        } else {
            let triangle=ray_triangle_at(first.triangle);let id=u32(triangle.a.w);
            if (triangle.surface_flags&16u)==0u&&id<arrayLength(&ray_materials) {
                if (ray_materials[id].flags&64u)!=0u {max_age=16.0;}
            }
        }
    }
    // Explicit fixed-scene diagnostic budget; default zero preserves every
    // interactive water/foliage/static limit above. No RNG or path changes.
    if ray_frame.previous_eye.w>=32.0 {
        max_age=min(ray_frame.previous_eye.w,256.0);
    }
    var age=1.0;
    let previous=ray_frame.previous*vec4f(position,1.0);
    let previous_uv=previous.xy/previous.w*vec2f(0.5,-0.5)+0.5;
    let history_size=vec2i(textureDimensions(ray_history));
    if replace_surface&&!primary_actor&&ray_frame.counts.w!=0u&&previous.w>0.0&&all(previous_uv>vec2f(0.0))&&all(previous_uv<vec2f(1.0)) {
        let hp=clamp(vec2i(previous_uv*vec2f(history_size)),vec2i(0),history_size-vec2i(1));
        let old=textureLoad(ray_history,hp,0);let old_geometry=textureLoad(ray_history_geometry,hp,0);
        let expected_depth=length(position-ray_frame.previous_eye.xyz);
        var compatible=false;
        if complete_path {
            if ray_frame.water.w>0.5 {
                let previous_pixel=min(hp*stride+vec2i(stride/2),size-vec2i(1));
                compatible=ray_water_history_compatible(old.a,old_geometry,geometric_normal,geometry_class,
                    position,ray_frame.previous,ray_frame.previous_eye.xyz,vec2f(previous_pixel),vec2f(size),depth_gradient);
            } else {
                compatible=old_geometry.z<-2.0&&abs(old_geometry.z-geometry_class)<0.20
                    &&ray_history_compatible(old.a,expected_depth,old_geometry,geometric_normal,abs(geometry_class),true,depth_gradient);
            }
        } else if old_geometry.z>=-2.0 {
            compatible=ray_history_compatible(old.a,expected_depth,old_geometry,geometric_normal,receiver.z,raster_water,depth_gradient);
        }
        if compatible {
            age=min(old_geometry.w+1.0,max_age);delta=mix(delta,old.rgb,1.0-1.0/age);
            if RAY_WATER_LOBES&&complete_path&&first_water&&!eye_water&&ray_frame.water.z<=0.5 {
                reflection=mix(reflection,textureLoad(ray_lobe_history,hp,0).rgb,1.0-1.0/age);
            }
        }
    }
    return RayTransportOutput(vec4f(delta,receiver.w),
        vec4f(oct_encode(geometric_normal),geometry_class,select(select(-1.0,age,replace_surface),-2.0,primary_actor)),
        ray_lobe_packet(reflection,select(-primary_t,primary_t,replace_surface)),vec4f(current,0.0));
}

@fragment fn fs_transport(@builtin(position) frag:vec4f)->RayTransportOutput {
    return ray_primary_transport(frag);
}
