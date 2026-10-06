// Bounded single scattering in the camera-local, actually rendered sun volume.
// The clear-air approximation is conservative: unknown or shadowed air adds no
// exterior light. It never assumes that a point outside the shadow map is lit.
struct AtmosphereUniform {
    inverse_projection: mat4x4f,
    shadow_projection: mat4x4f,
    eye: vec4f,
    sun: vec4f,
    radiance: vec4f,
    ambient: vec4f,
    settings: vec4f,
};
@group(0) @binding(0) var scene_depth: texture_depth_2d;
@group(0) @binding(1) var sun_depth: texture_depth_2d;
@group(0) @binding(2) var sun_sampler: sampler_comparison;
@group(0) @binding(3) var<uniform> atmosphere: AtmosphereUniform;
@group(0) @binding(4) var volume: texture_2d<f32>;
@group(0) @binding(5) var volume_distance: texture_2d<f32>;
@group(0) @binding(6) var surface_distance: texture_2d<f32>;

@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4f {
    let position = array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
    return vec4f(position[vertex],0.0,1.0);
}
fn atmosphere_world(pixel: vec2i, depth: f32) -> vec3f {
    let uv = (vec2f(pixel)+0.5)/vec2f(textureDimensions(scene_depth));
    let world = atmosphere.inverse_projection*vec4f(uv*vec2f(2.0,-2.0)+vec2f(-1.0,1.0),depth,1.0);
    return world.xyz/world.w;
}
// Transparent water keeps opaque depth intact. Its reflection record carries
// eye distance, so clear air ends at the water rather than below its surface.
fn atmosphere_endpoint(pixel: vec2i, depth: f32) -> vec3f {
    let world = atmosphere_world(pixel,depth);
    let delta = world-atmosphere.eye.xyz;
    let opaque_distance = length(delta);
    let surface = textureLoad(surface_distance,pixel,0);
    let receiving_distance = surface.w;
    if surface.z>0.0 && receiving_distance>0.0 && receiving_distance<opaque_distance {
        return atmosphere.eye.xyz+delta*(receiving_distance/max(opaque_distance,0.00001));
    }
    return world;
}
fn atmosphere_sun_visibility(world: vec3f) -> f32 {
    let p = atmosphere.shadow_projection*vec4f(world,1.0);
    let uv = p.xy*vec2f(0.5,-0.5)+0.5;
    if p.z<=0.0 || p.z>=1.0 || any(uv<=vec2f(0.0)) || any(uv>=vec2f(1.0)) { return 0.0; }
    let edge = min(min(uv.x,uv.y),min(1.0-uv.x,1.0-uv.y));
    // No surface bias in air: bias would admit sunlight below a cave roof.
    return textureSampleCompareLevel(sun_depth,sun_sampler,uv,p.z)
        *smoothstep(0.0,0.035,edge);
}
fn atmosphere_phase(cosine: f32) -> f32 {
    // Henyey-Greenstein, normalized over the sphere; forward-scattering dust.
    let g = 0.65;
    return (1.0-g*g)/(12.5663706*pow(max(1.0+g*g-2.0*g*cosine,0.001),1.5));
}
fn atmosphere_integrate(endpoint: vec3f) -> vec4f {
    let delta = endpoint-atmosphere.eye.xyz;
    let distance = length(delta);
    let ray = delta/max(distance,0.00001);
    let end = min(distance,atmosphere.settings.y);
    if end<=3.0 || atmosphere.settings.x<=0.0 { return vec4f(0.0); }
    let step = (end-3.0)/12.0;
    let phase = atmosphere_phase(dot(ray,atmosphere.sun.xyz));
    let source = atmosphere.radiance.xyz*phase*3.0+atmosphere.ambient.xyz*0.20*atmosphere.eye.w;
    var transmittance = 1.0;
    var scattered = vec3f(0.0);
    for (var sample = 0u; sample<12u; sample++) {
        let world = atmosphere.eye.xyz+ray*(3.0+(f32(sample)+0.5)*step);
        let visibility = atmosphere_sun_visibility(world);
        // A fixed world-space layer keeps density stable when the observer
        // climbs. Elevation 32 spans the terrain's broad valley/canopy band.
        let height = clamp(exp(-(world.y-32.0)*0.055),0.25,2.0);
        let extinction = 1.0-exp(-atmosphere.settings.x*height*visibility*step);
        scattered += transmittance*source*extinction;
        transmittance *= 1.0-extinction;
    }
    return vec4f(scattered,1.0-transmittance);
}
struct AtmosphereOutput { @location(0) scattering: vec4f, @location(1) distance: f32 };
@fragment fn integrate(@builtin(position) position: vec4f) -> AtmosphereOutput {
    let size = vec2i(textureDimensions(scene_depth));
    let origin = vec2i(position.xy)*2;
    var endpoint = atmosphere_endpoint(min(origin,size-1),textureLoad(scene_depth,min(origin,size-1),0));
    var distance = length(endpoint-atmosphere.eye.xyz);
    // Nearest surface in each 2x2 footprint avoids bright background scattering
    // leaking across foreground silhouettes before the bilateral resolve.
    for (var y=0; y<2; y++) {
        for (var x=0; x<2; x++) {
            let candidate = min(origin+vec2i(x,y),size-1);
            let z = textureLoad(scene_depth,candidate,0);
            let point = atmosphere_endpoint(candidate,z);
            let candidate_distance = length(point-atmosphere.eye.xyz);
            if candidate_distance<=distance { distance=candidate_distance; endpoint=point; }
        }
    }
    return AtmosphereOutput(atmosphere_integrate(endpoint),length(endpoint-atmosphere.eye.xyz));
}
@fragment fn composite(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let pixel = vec2i(position.xy);
    let depth = textureLoad(scene_depth,pixel,0);
    let distance = length(atmosphere_endpoint(pixel,depth)-atmosphere.eye.xyz);
    let low_position = position.xy*0.5-0.5;
    let origin = vec2i(floor(low_position));
    let blend = fract(low_position);
    let size = vec2i(textureDimensions(volume));
    var result = vec4f(0.0);
    var weight_sum = 0.0;
    for (var y=0; y<2; y++) {
        for (var x=0; x<2; x++) {
            let p = clamp(origin+vec2i(x,y),vec2i(0),size-1);
            let sample_distance = textureLoad(volume_distance,p,0).r;
            let difference = abs(sample_distance-distance);
            let spatial = select(1.0-blend.x,blend.x,x==1)*select(1.0-blend.y,blend.y,y==1);
            let weight = spatial*exp(-difference/max(0.5,distance*0.025));
            result += textureLoad(volume,p,0)*weight;
            weight_sum += weight;
        }
    }
    // A discontinuity with no matching low-resolution footprint receives no
    // fog, rather than borrowing the background's light through solid walls.
    return result/max(weight_sum,0.0001);
}
