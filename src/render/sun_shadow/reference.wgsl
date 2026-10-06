fn bg_shadow_project(world:vec3f)->vec4f {
    let p=bg_shadow.view_projection*vec4f(world,1.0);
    // Point-light caster maps use the same vertex entries but distinct camera
    // matrices and a negative sentinel. Their projection remains untouched.
    if bg_shadow.reference_params.x>0.5&&bg_shadow.params.z>=0.0 {
        let uvz=bg_bsl_distort_shadow(vec3f(p.xy,p.z*2.0-1.0),bg_shadow.reference_params.z);
        return vec4f((uvz.xy*2.0-1.0)*p.w,uvz.z*p.w,p.w);
    }
    return p;
}
fn bg_bsl_reference_sun_visibility(receiver:BgShadowReceiver,normal:vec3f,light_direction:vec3f,subsurface:f32,sky:f32)->f32 {
    if bg_shadow.reference_params.x<=0.5 {return bg_enhanced_sun_visibility(receiver);}
    if bg_shadow.params.z<=0.0 {return 1.0;}
    let no_l=clamp(dot(normal,light_direction),0.0,1.0);
    if no_l<=0.0&&subsurface<=0.0 {return 1.0;}
    let raw=vec3f(receiver.projected.xy,receiver.projected.z*2.0-1.0);
    let fade=clamp(100.0-100.0*max(abs(raw.x),abs(raw.y)),0.0,1.0)*clamp(sky*1000.0-1.0,0.0,1.0);
    if fade<0.00001 {return 1.0;}
    let uvz=bg_bsl_distort_shadow(raw,bg_shadow.reference_params.z);
    if uvz.z<=0.0||uvz.z>=1.0 {return 1.0;}
    let tap_params=bg_bsl_shadow_bias(raw,distance(receiver.world,bg_shadow.center.xyz),no_l,subsurface,bg_shadow.params.x,bg_shadow.reference_params.y,bg_shadow.reference_params.z);
    let taps=array<vec2f,9>(vec2f(0.0),vec2f(0.0,1.0),vec2f(0.7,0.7),vec2f(1.0,0.0),vec2f(0.7,-0.7),vec2f(0.0,-1.0),vec2f(-0.7,-0.7),vec2f(-1.0,0.0),vec2f(-0.7,0.7));
    var visibility=0.0;
    for(var i=0u;i<9u;i++) {
        visibility+=textureSampleCompareLevel(bg_shadow_depth,bg_shadow_sampler,uvz.xy*vec2f(1.0,-1.0)+vec2f(0.0,1.0)+taps[i]*tap_params.x,uvz.z-tap_params.y);
    }
    visibility/=9.0;
    // Current catalog renders only opaque/alpha-hole casters. Accepted opaque
    // texels have source shadowcolor=0; holes have no depth. RGB glass maps need
    // a real translucent material/render class, not botanical SSS metadata.
    let result=bg_bsl_shadow_result(visibility,vec3f(0.0),subsurface).x;
    return mix(1.0,result,fade);
}
fn bg_sun_visibility_material(receiver:BgShadowReceiver,normal:vec3f,sky:f32,subsurface:f32)->f32 {
    if bg_shadow.reference_params.x>0.5 {
        let direction=-normalize(vec3f(bg_shadow.view_projection[0].z,bg_shadow.view_projection[1].z,bg_shadow.view_projection[2].z));
        return bg_bsl_reference_sun_visibility(receiver,normal,direction,subsurface,sky);
    }
    return bg_sun_visibility(receiver);
}

// Shared/basic receiver compatibility. Explicit material callers provide their
// authoritative shading normal and lightmap instead of this geometric fallback.
fn bg_sun_visibility(receiver:BgShadowReceiver)->f32 {
    if bg_shadow.reference_params.x>0.5 {
        let geometric=receiver.geometric;
        let normal=select(-geometric,geometric,geometric.y>=0.0);
        let direction=-normalize(vec3f(bg_shadow.view_projection[0].z,bg_shadow.view_projection[1].z,bg_shadow.view_projection[2].z));
        return bg_bsl_reference_sun_visibility(receiver,normal,direction,0.0,1.0);
    }
    return bg_enhanced_sun_visibility(receiver);
}
