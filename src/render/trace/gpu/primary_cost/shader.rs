//! Runtime packet selection reuses two shader variants for every counter bank.
use crate::render::trace::tests::query_counters as counters;

pub(super) fn source(base: String, instrumented: bool) -> String {
    let mut source = counters::instrument(base, instrumented);
    source.push_str(PROBE);
    source
}
const PROBE: &str = r#"
struct PrimaryProbe {points:array<vec4u,16>,config:vec4u};
@group(3) @binding(0) var<uniform> primary_probe:PrimaryProbe;
struct PrimaryProbeOutput {@location(0) color:vec4f,@location(1) metadata:vec4f};
fn primary_probe_stored(value:vec4f)->vec4f {
 return vec4f(unpack2x16float(pack2x16float(value.xy)),unpack2x16float(pack2x16float(value.zw)));
}
@fragment fn fs_primary_probe(@builtin(position) pixel:vec4f)->PrimaryProbeOutput {
 let mode=primary_probe.config.x;
 if mode==9u {
  // Exact binary32 inputs and independently specified IEEE binary16 ties,
  // signs, normal/subnormal boundary and smallest subnormal in the host test.
  let cases=array<u32,16>(0x00000000u,0x80000000u,0x3f800000u,0xbf800000u,
   0x3f801000u,0x3f801001u,0x3f803000u,0x477fe000u,
   0x38800000u,0x33800000u,0x33000000u,0x33000001u,
   0xb3800000u,0xb3000000u,0x387fc000u,0xc0002000u);
  let value=bitcast<f32>(cases[(u32(pixel.x)+8u*u32(pixel.y))%16u]);
  let packed=pack2x16float(vec2f(value));
  return PrimaryProbeOutput(vec4f(value),vec4f(f32(packed&65535u),f32(packed>>16u),0.0,0.0));
 }
 let p=vec2u(pixel.xy);let block=p.x/2u+4u*(p.y/2u);
 let low=primary_probe.points[block].xy+p%vec2u(2u);
 // Packed blocks start on even framebuffer coordinates. Each true 2x2 quad
 // is adjacent in both frames, preserving mapped normal/wave/depth derivatives.
 let actual=vec4f(vec2f(low)+0.5,pixel.zw);
 let result=ray_primary_transport(actual);
 var first=vec4f(result.radiance.rgb,f32(probe_nodes));
 var second=vec4f(f32(probe_triangles),f32(probe_known),f32(probe_cloud),f32(probe_vertices));
 if mode==1u {
  first.w=f32(probe_regions);second=vec4f(f32(probe_directory),f32(probe_density),f32(probe_material),f32(probe_alpha_tests));
 } else if mode==2u {
  first.w=f32(probe_primary_lighting);second=vec4f(f32(probe_primary_medium),f32(probe_primary_split),f32(probe_guides),f32(probe_replays));
 } else if mode==3u {
  first.w=result.radiance.a;second=vec4f(f32(ray_rng&65535u),f32(ray_rng>>16u),result.geometry.zw);
 } else if mode==4u {first=result.geometry;second=result.transmission;}
 else if mode==5u {first=result.current;second=vec4f(f32(ray_rng&65535u),f32(ray_rng>>16u),0.0,0.0);}
 else if mode==6u {
  first=primary_probe_stored(result.radiance);
  // Never quantize the lossless RNG halves or raw class/age control values.
  second=vec4f(f32(ray_rng&65535u),f32(ray_rng>>16u),result.geometry.zw);
 } else if mode==7u {
  first=primary_probe_stored(result.geometry);second=primary_probe_stored(result.transmission);
 } else if mode==8u {
  first=primary_probe_stored(result.current);second=vec4f(f32(ray_rng&65535u),f32(ray_rng>>16u),0.0,0.0);
 }
 return PrimaryProbeOutput(first,second);
}
"#;
