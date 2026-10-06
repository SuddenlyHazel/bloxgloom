@group(2) @binding(0) var<storage,read> dyn_geometry:array<u32>;
@group(2) @binding(1) var<storage,read> dyn_assets:array<u32>;
@group(2) @binding(2) var<storage,read> dyn_frame:array<u32>;
const DYN_INSTANCE_WORDS:u32=44u;
const DYN_TRIANGLE_WORDS:u32=36u;
const DYN_NODE_WORDS:u32=12u;
fn dyn_af(i:u32)->f32 {return bitcast<f32>(dyn_assets[i]);}
fn dyn_ff(i:u32)->f32 {return bitcast<f32>(dyn_frame[i]);}
fn dyn_gf(i:u32)->f32 {return bitcast<f32>(dyn_geometry[i]);}
fn dyn_av3(i:u32)->vec3f {return vec3f(dyn_af(i),dyn_af(i+1u),dyn_af(i+2u));}
fn dyn_av4(i:u32)->vec4f {return vec4f(dyn_av3(i),dyn_af(i+3u));}
fn dyn_fv3(i:u32)->vec3f {return vec3f(dyn_ff(i),dyn_ff(i+1u),dyn_ff(i+2u));}
fn dyn_fv4(i:u32)->vec4f {return vec4f(dyn_fv3(i),dyn_ff(i+3u));}
fn dyn_gv3(i:u32)->vec3f {return vec3f(dyn_gf(i),dyn_gf(i+1u),dyn_gf(i+2u));}
fn dyn_gv4(i:u32)->vec4f {return vec4f(dyn_gv3(i),dyn_gf(i+3u));}
fn dyn_matrix(i:u32)->mat4x4f {return mat4x4f(dyn_fv4(i),dyn_fv4(i+4u),dyn_fv4(i+8u),dyn_fv4(i+12u));}
fn dyn_instance_for_triangle(index:u32)->u32 {
    var low=0u;var high=dyn_frame[0];
    loop {if low>=high {break;}let mid=(low+high)/2u;let at=8u+mid*DYN_INSTANCE_WORDS;
        if index>=dyn_frame[at+1u]+dyn_frame[at+3u] {low=mid+1u;}else{high=mid;}}
    return 8u+low*DYN_INSTANCE_WORDS;
}
