// Independent default AO_METHOD=0 / AO_STRENGTH=1 equations.
struct Camera {view_projection:mat4x4f,sun:vec4f,horizon:vec4f,eye:vec4f,fog_range:vec4f,parallax:vec4f,sun_radiance:vec4f,sky_zenith:vec4f,ambient_lower:vec4f,ambient_upper:vec4f,cloud:vec4f,ambient_sh:array<vec4f,6>};
struct Options {inverse:mat4x4f,size:vec4f,forward:vec4f};
@group(0) @binding(0) var depth:texture_depth_2d;
@group(0) @binding(1) var visibility:texture_2d<f32>;
@group(0) @binding(2) var noise:texture_2d<f32>;
@group(0) @binding(3) var pixels:sampler;
@group(0) @binding(4) var<uniform> options:Options;
@group(0) @binding(5) var<uniform> camera:Camera;
@group(0) @binding(6) var noise_pixels:sampler;
@vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[i],0.0,1.0);
}
fn bg_ao_depth(uv:vec2f)->f32 {
    let size=vec2i(textureDimensions(depth));
    return textureLoad(depth,clamp(vec2i(floor(uv*vec2f(size))),vec2i(0),size-1),0);
}
fn bg_ao_position(uv:vec2f,z:f32)->vec3f {
    let p=options.inverse*vec4f(uv*vec2f(2.0,-2.0)+vec2f(-1.0,1.0),z,1.0);return p.xyz/p.w;
}
fn bg_ao_linear(uv:vec2f)->f32 {return dot(bg_ao_position(uv,bg_ao_depth(uv)),options.forward.xyz);}
fn bg_ao_normal(uv:vec2f,center:vec3f,linear:f32)->vec3f {
    let px=vec2f(1.0,0.0)/options.size.xy;let py=vec2f(0.0,-1.0)/options.size.xy;
    let east=bg_ao_position(uv+px,bg_ao_depth(uv+px));let west=bg_ao_position(uv-px,bg_ao_depth(uv-px));
    let north=bg_ao_position(uv+py,bg_ao_depth(uv+py));let south=bg_ao_position(uv-py,bg_ao_depth(uv-py));
    let horizontal=select(center-west,east-center,abs(dot(east,options.forward.xyz)-linear)<abs(dot(west,options.forward.xyz)-linear));
    let vertical=select(center-south,north-center,abs(dot(north,options.forward.xyz)-linear)<abs(dot(south,options.forward.xyz)-linear));
    let normal=cross(horizontal,vertical);
    if dot(normal,normal)<1e-20 {return normalize(-center);}
    return normalize(normal);
}
@fragment fn sample_ao(@builtin(position) frag:vec4f)->@location(0) vec4f {
    let uv=frag.xy/options.size.xy;let z=bg_ao_depth(uv);
    if z>=1.0 {return vec4f(1.0);}
    let position=bg_ao_position(uv,z);let linear=dot(position,options.forward.xyz);
    let normal=bg_ao_normal(uv,position,linear);
    var dither=textureSampleLevel(noise,noise_pixels,vec2f(frag.x,options.size.y-frag.y)/512.0,0.0).b;
    if options.forward.w>0.5 {dither=fract(dither+options.size.w*0.618);}
    var current=0.2475*dither+0.01;
    let distance_scale=max(linear,2.5);let radius=0.25;
    let scale=radius*vec2f(options.size.y/options.size.x,1.0)*options.size.z/distance_scale;
    let difference_scale=linear/distance_scale;
    var direction=vec2f(cos(dither*6.28),sin(dither*6.28));
    let threshold=0.15+linear*0.01;
    var ao=0.0;var pointiness=0.0;
    for(var i=0u;i<4u;i++) {
        var offset=direction*current*scale;
        var visible=0.0;
        for(var j=0u;j<2u;j++) {
            let sample_uv=uv+offset*vec2f(1.0,-1.0);
            let difference=(bg_ao_position(sample_uv,bg_ao_depth(sample_uv))-position)/(radius*current*difference_scale);
            let attenuation=clamp(1.0+0.5/current-0.25*length(difference),0.0,1.0);
            // The engine has no Minecraft compressed hand-depth subrange;
            // native first-person meshes use normal physical depth instead.
            let angle=dot(normal,difference*inverseSqrt(max(dot(difference,difference),1e-30)))*(1.0+threshold);
            visible+=0.5-max(angle-threshold,0.0)*attenuation;
            pointiness+=max(-angle-threshold,0.0);
            offset=-offset;
        }
        ao+=clamp(visible,0.0,1.0);
        current+=0.2475;direction=vec2f(direction.x-direction.y,direction.x+direction.y)*0.7071;
    }
    return vec4f(mix(ao*0.25,1.0,pointiness*0.25),0.0,0.0,1.0);
}
fn bg_ao_fog_order(ao:f32,scatter:vec3f)->vec4f {
    return vec4f(scatter*(1.0-ao),ao);
}
@fragment fn composite_ao(@builtin(position) frag:vec4f)->@location(0) vec4f {
    let uv=frag.xy/options.size.xy;
    if bg_ao_depth(uv)>=1.0 {return vec4f(0.0,0.0,0.0,1.0);}
    let linear=bg_ao_linear(uv);
    let offsets=array<vec2f,4>(vec2f(1.5,0.5),vec2f(-0.5,1.5),vec2f(-1.5,-0.5),vec2f(0.5,-1.5));
    let depth_offsets=array<vec2f,4>(vec2f(2.0,1.0),vec2f(-1.0,2.0),vec2f(-2.0,-1.0),vec2f(1.0,-2.0));
    var total=0.0;var weight=0.0;
    for(var i=0u;i<4u;i++) {
        let sample_uv=uv+offsets[i]*vec2f(1.0,-1.0)/options.size.xy;
        let depth_uv=uv+depth_offsets[i]*vec2f(1.0,-1.0)/options.size.xy;
        let w=max(1.0-4.0*abs(linear-bg_ao_linear(depth_uv)),0.00001);
        total+=textureSampleLevel(visibility,pixels,sample_uv,0.0).r*w;weight+=w;
    }
    var ao=total/weight;
    if weight<0.0001 {ao=textureSampleLevel(visibility,pixels,uv,0.0).r;}
    let world=bg_ao_position(uv,bg_ao_depth(uv))+camera.eye.xyz;
    // Source applies full-color AO before fog. Raster already applied fog;
    // this algebra restores exactly AO*C*T + fog*(1-T), without inversion.
    var scatter=vec3f(0.0);
    if camera.fog_range.w>=-0.5 && camera.fog_range.w<=0.5 {
        scatter=bg_fog_air_radiance(world)*bg_reference_fog_amount(world);
    }
    return bg_ao_fog_order(ao,scatter);
}
