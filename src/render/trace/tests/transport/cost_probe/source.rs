//! Secondary entrypoints share the test-only production counter hooks.
use crate::render::trace::tests::query_counters as counters;

pub(super) fn source(instrumented: bool) -> String {
    // Explicit scalar production assembly: the vector-load experiment cannot
    // change these hooks. No fixture-only scatter or medium control branches.
    let mut source = crate::render::trace::gpu::transport_probe_source();
    for page in 0..4 {
        let declaration = format!("var<storage,read> ray_lod_page_{page}:array<u32>;");
        assert!(
            source.contains(&declaration),
            "counter probe requires scalar LOD pages"
        );
    }
    source.push_str(PROBE);
    counters::instrument(source, instrumented)
}

const PROBE: &str = r#"
fn probe_query(index:u32)->array<vec4f,2> {
 let at=ray_frame.counts.y+index*8u;
 return array<vec4f,2>(bitcast<vec4f>(vec4u(ray_volume_word(at),ray_volume_word(at+1u),ray_volume_word(at+2u),ray_volume_word(at+3u))),
  bitcast<vec4f>(vec4u(ray_volume_word(at+4u),ray_volume_word(at+5u),ray_volume_word(at+6u),ray_volume_word(at+7u))));
}
struct ProbeOutput {@location(0) color:vec4f,@location(1) counts:vec4f};
fn probe_path(pixel:vec4f,rng_output:bool)->ProbeOutput {
 let query=probe_query(u32(pixel.x));
 ray_rng=u32(pixel.x)*1973u+u32(pixel.y)*26699u+911u;
 let hdr=transport(query[0].xyz,query[1].xyz,query[0].w);
 var counters=vec4f(f32(probe_triangles),f32(probe_known),f32(probe_cloud),f32(probe_vertices));
 // Every16-bit integer survives Float32 storage exactly. Bitcasting a random
 // u32 to float could produce a NaN that the color store canonicalizes.
 if rng_output {counters=vec4f(f32(ray_rng&65535u),f32(ray_rng>>16u),counters.zw);}
 return ProbeOutput(vec4f(hdr,f32(probe_nodes)),counters);
}
@fragment fn fs_probe(@builtin(position) pixel:vec4f)->ProbeOutput {return probe_path(pixel,false);}
@fragment fn fs_rng(@builtin(position) pixel:vec4f)->ProbeOutput {return probe_path(pixel,true);}
"#;
