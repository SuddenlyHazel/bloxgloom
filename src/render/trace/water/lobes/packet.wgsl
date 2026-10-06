const RAY_WATER_LOBES:bool=false;
@group(0) @binding(15) var ray_lobe_history:texture_2d<f32>;
fn ray_primary_t(packet:vec4f)->f32 {return select(packet.r,packet.w,RAY_WATER_LOBES);}
fn ray_lobe_packet(reflection:vec3f,transmission:f32)->vec4f {
    if RAY_WATER_LOBES {return vec4f(reflection,transmission);}
    return vec4f(transmission,0.0,0.0,0.0);
}
