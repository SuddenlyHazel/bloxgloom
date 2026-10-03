struct Settings { inverse: mat4x4f, projection: mat4x4f, size_radius_strength: vec4f, bias: vec4f };
@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var indirect: texture_2d<f32>;
@group(0) @binding(2) var visibility: texture_2d<f32>;
@group(0) @binding(3) var<uniform> settings: Settings;
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let p = array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
    return vec4f(p[index],0.0,1.0);
}
fn in_bounds(p: vec2i) -> bool { return all(p >= vec2i(0)) && all(p < vec2i(settings.size_radius_strength.xy)); }
fn unproject(p: vec2f, z: f32) -> vec3f {
    let uv = p / settings.size_radius_strength.xy;
    let h = settings.inverse * vec4f(uv * vec2f(2.0,-2.0) + vec2f(-1.0,1.0), z, 1.0);
    return h.xyz / h.w;
}
fn position(p: vec2i) -> vec3f { return unproject(vec2f(p)+0.5,textureLoad(depth,p,0)); }
// Pick the derivative on the shallower discontinuity, preventing silhouette
// normals from pointing across empty space. It works for any depth-writing mesh.
fn normal_at(p: vec2i, center: vec3f) -> vec3f {
    let edge = vec2i(settings.size_radius_strength.xy)-1;
    let l = position(clamp(p-vec2i(1,0),vec2i(0),edge));
    let r = position(clamp(p+vec2i(1,0),vec2i(0),edge));
    let u = position(clamp(p-vec2i(0,1),vec2i(0),edge));
    let d = position(clamp(p+vec2i(0,1),vec2i(0),edge));
    let dx = select(r-center,center-l,distance(center,l)<distance(center,r));
    let dy = select(d-center,center-u,distance(center,u)<distance(center,d));
    let toward_eye = unproject(vec2f(p)+0.5,0.0)-center;
    let n = cross(dx,dy);
    if dot(n,n) < 0.000000000001 { return normalize(toward_eye); }
    return normalize(n) * select(-1.0,1.0,dot(n,toward_eye)>=0.0);
}
@fragment fn horizon(@builtin(position) frag: vec4f) -> @location(0) f32 {
    let p = vec2i(frag.xy);
    if textureLoad(depth,p,0) >= 0.999999 || all(textureLoad(indirect,p,0).rgb <= vec3f(0.000001)) { return 1.0; }
    let center = position(p);
    let n = normal_at(p,center);
    let radius = settings.size_radius_strength.z;
    // Obtain pixel scale from the actual projection, including TAA jitter and
    // orthographic fixtures. Euclidean derivative lengths preserve world units.
    let dx = length(unproject(frag.xy+vec2f(1.0,0.0),textureLoad(depth,p,0))-center);
    let dy = length(unproject(frag.xy+vec2f(0.0,1.0),textureLoad(depth,p,0))-center);
    let pixels = min(vec2f(96.0),vec2f(radius)/max(vec2f(dx,dy),vec2f(0.00001)));
    if max(pixels.x,pixels.y)<1.5 { return 1.0; }
    var total = 0.0;
    // Fixed rotated spokes avoid frame-random noise and temporal ghosting.
    for (var direction=0u; direction<8u; direction++) {
        let angle = (f32(direction)+0.125) * 0.785398163;
        let ray = vec2f(cos(angle),sin(angle));
        var horizon_height = 0.0;
        for (var step=1u; step<=4u; step++) {
            let fraction = f32(step)*0.25;
            let sample_pixel = p + vec2i(round(ray * pixels * fraction * fraction));
            if !in_bounds(sample_pixel) || all(sample_pixel==p) { continue; }
            if textureLoad(depth,sample_pixel,0)>=0.999999 { continue; }
            let delta = position(sample_pixel)-center;
            let squared = dot(delta,delta);
            if squared<0.000001 || squared>=radius*radius { continue; }
            let cosine = max(0.0,dot(n,delta)*inverseSqrt(squared)-settings.bias.x);
            let falloff = 1.0-squared/(radius*radius);
            horizon_height = max(horizon_height,cosine*falloff);
        }
        total += horizon_height;
    }
    return clamp(1.0-settings.size_radius_strength.w*total*0.25,0.15,1.0);
}
@fragment fn subtract_indirect(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let p=vec2i(frag.xy);
    let energy=textureLoad(indirect,p,0);
    if textureLoad(depth,p,0)>=0.999999 || all(energy.rgb<=vec3f(0.000001)) { return vec4f(0.0); }
    let center=position(p);
    let n=normal_at(p,center);
    var sum=0.0; var weights=0.0;
    // Small bilateral gather filters spokes without bleeding across silhouettes
    // or neighboring depth planes. No temporal accumulation means moving
    // geometry has no AO history to trail behind it.
    for(var y=-1;y<=1;y++) { for(var x=-1;x<=1;x++) {
        let q=p+vec2i(x,y);
        if !in_bounds(q) || textureLoad(depth,q,0)>=0.999999 { continue; }
        let delta=position(q)-center;
        let plane=abs(dot(delta,n));
        let weight=exp(-plane*plane/0.0025) / (1.0+f32(x*x+y*y));
        sum+=textureLoad(visibility,q,0).x*weight; weights+=weight;
    }}
    let ao=sum/max(weights,0.00001);
    // Alpha magnitude is baked/contact visibility. Negative alpha is reserved
    // for untracked-motion reactivity; it has the same lighting interpretation.
    let additional=max(0.0,abs(energy.a)-ao);
    return vec4f(max(energy.rgb,vec3f(0.0))*additional,0.0);
}
