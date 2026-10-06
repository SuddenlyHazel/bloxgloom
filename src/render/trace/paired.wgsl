// Accumulate baseline-independent illumination. Current retained ambient,
// raster HDR extinction and specular fallback are cancelled at reconstruction.
struct RayLightingSample {radiance:vec3f,transmission:f32,reflection:vec3f};
fn ray_lighting_sample(complete_path:bool,first_water:bool,eye_water:bool,
    valid_opaque:bool,raster_water:bool,media_only:bool,first:RayHit,
    first_position:vec3f,first_footprint:vec4f,direction:vec3f,
    opaque_distance:f32,response:vec4f,receiver:vec4f,n:vec3f,
    position:vec3f,raster_indirect:vec4f)->RayLightingSample {
    var delta=vec3f(0.0);var primary_t=1.0;var reflection=vec3f(0.0);
    if complete_path {
        // Positive complete radiance is filtered independently; the exact
        // full-resolution raster baseline is removed only during reconstruction.
        if first_water&&!eye_water {
            var water_radiance=vec3f(0.0);
            if RAY_WATER_LOBES&&ray_frame.water.z<=0.5 {
                let lobes=ray_primary_water_lobes(first,first_position,direction,first_footprint);
                water_radiance=lobes.total;reflection=lobes.reflection;
            } else {
                water_radiance=ray_primary_water_radiance(first,first_position,direction,first_footprint);
            }
            // Camera air is deterministic extinction plus sampled in-scattering.
            // Branch continuations start at the interface, so this segment is
            // integrated once, not independently randomized for each lobe.
            let medium=ray_primary_medium_sample(ray_frame.eye.xyz,direction,first.distance,vec3f(0.0),water_radiance);
            delta=medium.correction;reflection*=medium.transmission;
        } else {
            delta=transport_limit(ray_frame.eye.xyz,direction,response.w,opaque_distance+0.1,true,first_footprint);
        }
    } else {
        if valid_opaque {
            var surface=ray_surface(first,position);surface.pbr.roughness=receiver.z;
            let sample=scatter(n,-direction,surface,position);
            if any(sample.weight!=vec3f(0.0)) {
                delta=sample.weight*transport(position+first.normal*select(-0.01,0.01,dot(sample.direction,first.normal)>=0.0),sample.direction,surface.sky);
            }
        }
        let medium=ray_primary_medium_sample(ray_frame.eye.xyz,direction,receiver.w,vec3f(0.0),delta);
        delta=medium.correction;primary_t=medium.transmission;
        delta/=select(ray_filter_basis(raster_indirect),vec3f(1.0),raster_water||media_only);
    }
    return RayLightingSample(delta,primary_t,reflection);
}
