struct Settings { inverse_current:mat4x4f,previous:mat4x4f,params:vec4f,depth_range:vec4f };
@group(0) @binding(0) var current:texture_2d<f32>;
@group(0) @binding(1) var depth:texture_depth_2d;
@group(0) @binding(2) var history:texture_2d<f32>;
@group(0) @binding(3) var history_depth:texture_2d<f32>;
@group(0) @binding(4) var linear_sampler:sampler;
@group(0) @binding(5) var<uniform> settings:Settings;
@group(0) @binding(6) var object_motion:texture_2d<f32>;
@group(0) @binding(7) var reactive:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position) vec4f {let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));return vec4f(p[id],0.0,1.0);}
fn linear_depth(z:f32)->f32 {return settings.depth_range.x/max(1.0-z*(1.0-settings.depth_range.x/settings.depth_range.y),0.000001);}
fn ycocg(c:vec3f)->vec3f {return vec3f(dot(c,vec3f(0.25,0.5,0.25)),(c.r-c.b)*0.5,dot(c,vec3f(-0.25,0.5,-0.25)));}
fn rgb(c:vec3f)->vec3f {let n=c.x-c.z;return vec3f(n+c.y,c.x+c.z,n-c.y);}
fn clip(value:vec3f,lo:vec3f,hi:vec3f)->vec3f {let center=(lo+hi)*0.5;let delta=value-center;let unit=abs(delta/((hi-lo)*0.5+0.00000001));return center+delta/max(1.0,max(unit.x,max(unit.y,unit.z)));}
fn current_sample(uv:vec2f)->vec3f {return textureSampleLevel(current,linear_sampler,uv,0.0).rgb;}
fn missing_history(uv:vec2f,view:vec2f)->vec3f {let d=view*0.1667;return (current_sample(uv-d)+current_sample(uv+d)+current_sample(uv+vec2f(d.x,-d.y))+current_sample(uv+vec2f(-d.x,d.y)))*0.25;}
// Source five-tap normalized Catmull-Rom, c=.7; not the usual c=.5.
fn history_sample(uv:vec2f,size:vec2f)->vec3f {
    let position=uv*size;let center=floor(position-0.5)+0.5;let f=position-center;
    let f2=f*f;let f3=f*f2;let c=0.7;
    let w0=-c*f3+2.0*c*f2-c*f;let w1=(2.0-c)*f3-(3.0-c)*f2+1.0;
    let w2=-(2.0-c)*f3+(3.0-2.0*c)*f2+c*f;let w3=c*f3-c*f2;let w12=w1+w2;
    let tc12=(center+w2/w12)/size;let tc0=(center-1.0)/size;let tc3=(center+2.0)/size;
    var total=vec3f(0.0);var weight=0.0;
    let offsets=array<vec2f,5>(vec2f(tc12.x,tc0.y),vec2f(tc0.x,tc12.y),tc12,vec2f(tc3.x,tc12.y),vec2f(tc12.x,tc3.y));
    let weights=array<f32,5>(w12.x*w0.y,w0.x*w12.y,w12.x*w12.y,w3.x*w12.y,w12.x*w3.y);
    for(var i=0u;i<5u;i++){total+=textureSampleLevel(history,linear_sampler,offsets[i],0.0).rgb*weights[i];weight+=weights[i];}
    return total/weight;
}
struct Output {@location(0) color:vec4f,@location(1) depth:f32};
@fragment fn resolve(@builtin(position) position:vec4f)->Output {
    let size=vec2i(textureDimensions(current));let pixel=vec2i(position.xy);let uv=position.xy/vec2f(size);let view=1.0/vec2f(size);
    let z=textureLoad(depth,pixel,0);let motion=textureLoad(object_motion,pixel,0);
    let marked=motion.a<0.0||textureLoad(reactive,pixel,0).a<0.0;
    let sky=z>=1.0;let color=textureLoad(current,pixel,0).rgb;
    var out=Output(vec4f(color,1.0),select(linear_depth(z),-1.0,sky&&!marked));
    if marked {return out;}
    if settings.params.y<0.5 {out.color=vec4f(missing_history(uv,view),1.0);return out;}
    var previous_uv:vec2f;var expected:f32;
    if sky {
        // Engine sky is unjittered. Remove translation and raster jitter.
        let ndc=(uv+settings.depth_range.zw)*vec2f(2.0,-2.0)+vec2f(-1.0,1.0);
        let near=settings.inverse_current*vec4f(ndc,0.0,1.0);let far=settings.inverse_current*vec4f(ndc,1.0,1.0);
        let previous=settings.previous*vec4f(normalize(far.xyz/far.w-near.xyz/near.w),0.0);
        if previous.w<=0.0 {return out;}
        previous_uv=previous.xy/previous.w*vec2f(0.5,-0.5)+0.5;expected=-1.0;
    } else if motion.a>0.0 {previous_uv=uv+motion.rg/vec2f(size);expected=motion.b;}
    else {
        let world=settings.inverse_current*vec4f(uv*vec2f(2.0,-2.0)+vec2f(-1.0,1.0),z,1.0);
        let previous=settings.previous*world;if previous.w<=0.0 {return out;}
        let ndc=previous.xyz/previous.w;if ndc.z<0.0||ndc.z>=1.0 {return out;}
        previous_uv=ndc.xy*vec2f(0.5,-0.5)+0.5+settings.depth_range.zw;expected=linear_depth(ndc.z);
    }
    if any(previous_uv<=vec2f(0.0))||any(previous_uv>=vec2f(1.0)) {return out;}
    // Catmull-Rom has negative lobes and a 4×4 footprint. Reject mixed-class or
    // disoccluded history taps, including those outside its central bilinear box.
    let base=vec2i(floor(previous_uv*vec2f(size)-0.5));
    for(var y=-1;y<=2;y++){for(var x=-1;x<=2;x++) {
        let saved=textureLoad(history_depth,clamp(base+vec2i(x,y),vec2i(0),size-1),0).r;
        if sky {if saved>=0.0 {return out;}}
        else {if saved<0.0||abs(saved-expected)>max(0.02,expected*settings.params.z) {return out;}}
    }}
    let old=history_sample(previous_uv,vec2f(size));
    if all(old==vec3f(0.0)) {out.color=vec4f(missing_history(uv,view),1.0);return out;}
    var lo=ycocg(color);var hi=lo;
    for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++) {
        let p=clamp(pixel+vec2i(x,y),vec2i(0),size-1);
        let sample=ycocg(textureLoad(current,p,0).rgb);lo=min(lo,sample);hi=max(hi,sample);
    }}
    let old_clipped=rgb(clip(ycocg(old),lo,hi));
    let blend=0.7+0.2*exp(-length((uv-previous_uv)*vec2f(size)));
    out.color=vec4f(mix(color,old_clipped,blend),1.0);return out;
}
