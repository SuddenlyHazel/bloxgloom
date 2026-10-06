@group(0) @binding(9) var ray_scene:texture_2d<f32>;
fn uniform_sphere()->vec3f {
    let z=random()*2.0-1.0;let a=random()*2.0*RAY_PI;
    return vec3f(sqrt(max(0.0,1.0-z*z))*cos(a),z,sqrt(max(0.0,1.0-z*z))*sin(a));
}
// Stable elementary functions retain weak air extinction without cancellation.
fn ray_exponential_opacity(depth:f32)->f32 {
    if abs(depth)<0.001 {return depth*(1.0-depth*(0.5-depth*(1.0/6.0-depth/24.0)));}
    return 1.0-exp(-depth);
}
fn ray_negative_log_one_minus(x:f32)->f32 {
    if abs(x)<0.001 {return x*(1.0+x*(0.5+x*(1.0/3.0+x*0.25)));}
    return -log(1.0-x);
}
fn ray_log_one_plus(x:f32)->f32 {
    if x<0.001 {return x*(1.0-x*(0.5-x*(1.0/3.0-x*0.25)));}
    return log(1.0+x);
}
// Exact depth for sigma(y)=sigma0*exp(-max(y-20,0)/90). Integrate the
// constant lower segment and exponential upper segment in either ray direction.
fn ray_air_depth(height:f32,vertical:f32,distance:f32)->f32 {
    let sigma=max(ray_frame.parameters.x,0.0);
    let end=height+vertical*distance;
    if vertical==0.0 {return sigma*exp(-max(height-20.0,0.0)/90.0)*distance;}
    let near=min(height,end);let far=max(height,end);
    let below=clamp((20.0-near)/abs(vertical),0.0,distance);
    let above=distance-below;
    let span=abs(vertical)*above/90.0;
    var average=1.0;
    if span>0.0 {average=ray_exponential_opacity(span)/span;}
    return sigma*below+sigma*exp(-max(near-20.0,0.0)/90.0)*above*average;
}
// Invert one segment that does not cross height20. Log-space softplus handles
// descending rays whose density at a very high origin underflows to zero.
fn ray_air_segment_distance(height:f32,vertical:f32,distance:f32,depth:f32)->f32 {
    let sigma=ray_frame.parameters.x;
    if depth<=0.0||sigma<=0.0 {return 0.0;}
    if height+vertical*distance*0.5<=20.0 {return min(distance,depth/sigma);}
    let log_sigma=log(sigma)-max(height-20.0,0.0)/90.0;
    if vertical==0.0 {return min(distance,depth/exp(log_sigma));}
    let rate=vertical/90.0;
    if rate<0.0 {
        let x=log(depth*(-rate))-log_sigma;
        let integral=max(x,0.0)+ray_log_one_plus(exp(-abs(x)));
        return clamp(integral/(-rate),0.0,distance);
    }
    let local_sigma=exp(log_sigma);
    let fraction=depth*rate/local_sigma;
    if fraction<0.8 {
        return clamp(ray_negative_log_one_minus(fraction)/rate,0.0,distance);
    }
    // Near the upper asymptote use the remaining depth plus endpoint density;
    // subtracting fraction from1 would erase very long, rare sampled distances.
    let remaining=max(ray_air_depth(height,vertical,distance)-depth,0.0);
    let ratio=exp(-rate*distance)+remaining*rate/local_sigma;
    if ratio<=0.0 {return distance;}
    return clamp(-log(ratio)/rate,0.0,distance);
}
fn ray_air_distance(height:f32,vertical:f32,distance:f32,depth:f32)->f32 {
    var first=distance;
    if vertical!=0.0 {
        let crossing=(20.0-height)/vertical;
        if crossing>0.0&&crossing<distance {first=crossing;}
    }
    let first_depth=ray_air_depth(height,vertical,first);
    if depth<=first_depth {return ray_air_segment_distance(height,vertical,first,depth);}
    return first+ray_air_segment_distance(height+vertical*first,vertical,distance-first,depth-first_depth);
}
// Cloud segments retain ordered quadrature/reservoir sampling. Cloud-free air
// uses its exact integral and conditional inverse CDF. Thin volume continuation
// is sampled with bounded probability; reciprocal weighting preserves energy.
struct RayPrimaryMedium {correction:vec3f,transmission:f32};
fn ray_primary_medium_sample(origin:vec3f,direction:vec3f,distance:f32,color:vec3f,correction:vec3f)->RayPrimaryMedium {
    let interval=bg_cloud_interval(origin,direction,distance);
    let cloud=interval.y>interval.x;
    var transmission=1.0;var opacity=0.0;var point=origin;
    if cloud {
        for(var i=0u;i<64u;i++) {
            var step=distance/64.0;var offset=(f32(i)+0.5)*step;
            if i<16u {step=interval.x/16.0;offset=(f32(i)+0.5)*step;}
            else if i<48u {step=(interval.y-interval.x)/32.0;offset=interval.x+(f32(i-16u)+0.5)*step;}
            else {step=(distance-interval.y)/16.0;offset=interval.y+(f32(i-48u)+0.5)*step;}
            let position=origin+direction*offset;
            let local=exp(-medium_density(position)*step);
            let contribution=transmission*(1.0-local);
            opacity+=contribution;
            if opacity>0.0 && random()*opacity<contribution {point=position;}
            transmission*=local;
        }
    } else {
        let depth=ray_air_depth(origin.y,direction.y,distance);
        transmission=exp(-depth);opacity=ray_exponential_opacity(depth);
    }
    let deterministic=correction*transmission-color*opacity;
    if opacity<=0.0 {return RayPrimaryMedium(deterministic,transmission);}
    let probability=min(1.0,opacity*32.0);
    if probability<1.0&&random()>=probability {return RayPrimaryMedium(deterministic,transmission);}
    if !cloud {
        let optical_sample=ray_negative_log_one_minus(min(random(),0.9999999)*opacity);
        point=origin+direction*ray_air_distance(origin.y,direction.y,distance,optical_sample);
    }
    let g=medium_anisotropy(point);
    let scattered=(sun_light(point)*medium_phase(dot(direction,ray_frame.sun.xyz),g)+transport(point,medium_direction(direction,g),1.0))*0.92;
    return RayPrimaryMedium(deterministic+(opacity/probability)*scattered,transmission);
}

fn ray_primary_medium(origin:vec3f,direction:vec3f,distance:f32,color:vec3f,correction:vec3f)->vec3f {
    return ray_primary_medium_sample(origin,direction,distance,color,correction).correction;
}
