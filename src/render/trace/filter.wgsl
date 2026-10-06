@group(0) @binding(0) var radiance:texture_2d<f32>;
@group(0) @binding(1) var receiver:texture_2d<f32>;
struct RayFrame {
    inverse:mat4x4f,previous:mat4x4f,eye:vec4f,sun:vec4f,solar:vec4f,
    horizon:vec4f,zenith:vec4f,cloud:vec4f,climate:vec4f,parameters:vec4f,previous_eye:vec4f,counts:vec4u,
};
@group(0) @binding(2) var<uniform> ray_frame:RayFrame;
@group(0) @binding(3) var geometry:texture_2d<f32>;
@group(0) @binding(4) var indirect:texture_2d<f32>;
@group(0) @binding(5) var depth:texture_depth_2d;
@group(0) @binding(6) var response:texture_2d<f32>;
@group(0) @binding(7) var transmission:texture_2d<f32>;
struct Output { @builtin(position) position:vec4f };
@vertex fn vs_main(@builtin(vertex_index) i:u32)->Output {
    let xy=vec2f(f32((i<<1u)&2u),f32(i&2u));return Output(vec4f(xy*2.0-1.0,0.0,1.0));
}
fn world_position(pixel:vec2f,distance:f32,size:vec2f)->vec3f {
    let ndc=(pixel+0.5)/size*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
    let far=ray_frame.inverse*vec4f(ndc,1.0,1.0);
    return ray_frame.eye.xyz+normalize(far.xyz/far.w-ray_frame.eye.xyz)*distance;
}
// Camera-path medium radiance lives on directions/segment lengths, not receiver
// planes. In particular equal-distance sky endpoints form a sphere. Preserve
// raster sky/foreground classification and finite segment-depth discontinuities.
fn ray_media_spatial_weight(center_direction:vec3f,sample_direction:vec3f,
    center_distance:f32,sample_distance:f32,pixel_distance_squared:f32,
    center_sky:bool,sample_sky:bool)->f32 {
    if center_sky!=sample_sky {return 0.0;}
    var segment=1.0;
    if !center_sky {
        let tolerance=max(0.03,center_distance*0.01);
        let delta=(center_distance-sample_distance)/tolerance;
        segment=exp(-delta*delta);
    }
    // Angular bandwidth ~14 degrees; screen-space support stays bounded by the
    // existing low-resolution radius and Gaussian, independent of radial range.
    let angular=exp(-max(0.0,1.0-dot(center_direction,sample_direction))*32.0);
    return segment*angular*exp(-pixel_distance_squared/9.0);
}
// Irradiance-like corrections are denoised at transport resolution. Keep the
// center radial depth unchanged; transmission and material detail are separate.
@fragment fn fs_filter(@builtin(position) frag:vec4f)->@location(0) vec4f {
    let p=vec2i(frag.xy);let center=textureLoad(radiance,p,0);
    let center_geometry=textureLoad(geometry,p,0);
    if center.a<=0.0||abs(center_geometry.w)<=0.0 {return vec4f(0.0);}
    let stride=max(2,i32(ray_frame.parameters.z));
    let size=vec2i(textureDimensions(radiance));let full_size=vec2i(textureDimensions(receiver));
    let full_pixel=min(p*stride+vec2i(stride/2),full_size-vec2i(1));
    let center_position=world_position(vec2f(full_pixel),center.a,vec2f(full_size));
    let n=oct_decode(center_geometry.xy);let water=center_geometry.z<0.0;
    let media_only=center_geometry.z>1.0;let roughness=abs(center_geometry.z);
    let center_sky=media_only&&textureLoad(depth,full_pixel,0)>=0.999999;
    let tolerance=max(0.012,center.a*0.0015);
    let radius=select(select(1,2,roughness>0.35),4,media_only);
    var sum=vec3f(0.0);var weights=0.0;
    for(var y=-4;y<=4;y++) {for(var x=-4;x<=4;x++) {
        if abs(x)>radius||abs(y)>radius {continue;}
        let q=p+vec2i(x,y);if any(q<vec2i(0))||any(q>=size) {continue;}
        let r=textureLoad(radiance,q,0);let g=textureLoad(geometry,q,0);
        if r.a<=0.0||abs(g.w)<=0.0||(g.w<0.0)!=(center_geometry.w<0.0)
            ||(g.z<0.0)!=water||(g.z>1.0)!=media_only||abs(abs(g.z)-roughness)>0.20 {continue;}
        let gn=oct_decode(g.xy);if dot(n,gn)<0.85 {continue;}
        let sample_pixel=min(q*stride+vec2i(stride/2),full_size-vec2i(1));
        let sample_position=world_position(vec2f(sample_pixel),r.a,vec2f(full_size));
        let offset=vec2f(q-p);
        var weight=0.0;
        if media_only {
            let sample_sky=textureLoad(depth,sample_pixel,0)>=0.999999;
            weight=ray_media_spatial_weight(n,gn,center.a,r.a,dot(offset,offset),center_sky,sample_sky);
        } else {
            weight=ray_spatial_weight(center_position,sample_position,gn,roughness,abs(g.z),dot(offset,offset),tolerance);
        }
        if radius==1 {weight*=exp(-dot(offset,offset)*1.5);}
        sum+=r.rgb*weight;weights+=weight;
    }}
    return vec4f(sum/max(weights,0.00001),center.a);
}
