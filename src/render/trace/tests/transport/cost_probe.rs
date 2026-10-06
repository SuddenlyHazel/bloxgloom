//! Opt-in query attribution, not a frame benchmark or a raster-primary fixture.
//! Real generated coast, production near/LOD admission and unchanged finite
//! secondary transport. Counters are private to each fragment; no atomics.
use super::*;
#[path = "cost_probe/scene.rs"]
mod coast;
#[path = "cost_probe/draw.rs"]
mod draw;
#[path = "cost_probe/source.rs"]
mod shader;

#[test]
fn cost_probe_source_has_exact_hooks_and_validates() {
    for instrumented in [false, true] {
        let source = shader::source(instrumented);
        let module = wgpu::naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}

#[test]
#[ignore = "real 512m coast query attribution; BLOXGLOOM_GI=1, exclusive GPU; not a timing benchmark"]
fn gpu_real_coast_fixed_paths_query_counts_preserve_hdr_and_rng() {
    assert_eq!(std::env::var("BLOXGLOOM_GI").as_deref(), Ok("1"));
    assert!(!render::bsl_reference::enabled());
    let catalog = crate::content::catalog();
    let fixture = Fixture::new(catalog);
    let scene = coast::build(&fixture, catalog);
    let original = shader::source(false);
    let counted = shader::source(true);
    let baseline = draw::run(&fixture, &scene.group, &original, "fs_rng");
    let rng = draw::run(&fixture, &scene.group, &counted, "fs_rng");
    let costs = draw::run(&fixture, &scene.group, &counted, "fs_probe");
    let mut totals = [0u64; 5];
    let mut maxima = [0u32; 5];
    for (index, ((old, new), counts)) in baseline.iter().zip(&rng).zip(&costs).enumerate() {
        for channel in 0..3 {
            assert_eq!(
                old[0][channel].to_bits(),
                new[0][channel].to_bits(),
                "counter instrumentation changed HDR or final RNG at fixed path {index}: {old:?}/{new:?}"
            );
        }
        for channel in 0..2 {
            assert_eq!(
                old[1][channel], new[1][channel],
                "counter instrumentation changed final RNG half at path {index}"
            );
            assert!(
                old[1][channel].is_finite()
                    && old[1][channel].fract() == 0.0
                    && (0.0..=65535.0).contains(&old[1][channel])
            );
        }
        let final_rng = old[1][0] as u32 | ((old[1][1] as u32) << 16);
        assert!(counts[0][..3].iter().all(|value| value.is_finite()));
        assert_eq!(old[0][..3], counts[0][..3]);
        let values = [
            counts[0][3],
            counts[1][0],
            counts[1][1],
            counts[1][2],
            counts[1][3],
        ];
        for (i, value) in values.into_iter().enumerate() {
            assert!(value.is_finite() && value >= 0.0 && value.fract() == 0.0);
            // Float32 MRT counts are exact only below 2^24; fail rather than
            // silently presenting rounded attribution as exact counts.
            assert!(
                value < 16_777_216.0,
                "counter exceeds exact Float32 integer range"
            );
            totals[i] += value as u64;
            maxima[i] = maxima[i].max(value as u32);
        }
        println!(
            "coast path={} seed={} HDR={:?} rng={:#010x} nodes={} triangles={} known_steps={} cloud_proposals={} vertices={}",
            index % coast::RAYS,
            index / coast::RAYS,
            &counts[0][..3],
            final_rng,
            values[0],
            values[1],
            values[2],
            values[3],
            values[4]
        );
    }
    assert!(totals[0] > 0 && totals[1] > 0 && totals[4] > 0);
    println!(
        "SCALAR LOD SECONDARY PATH ATTRIBUTION ONLY: generator={} seed={:#x}; real near={} triangles/{} nodes, selected LOD={} tiles/{} pages/{} triangles; {} fixed rays x {} seeds; totals(nodes,triangles,known_DDA,cloud_delta,vertices)={totals:?}; maxima={maxima:?}. No raster attachment, primary split, paired replay, history, filter, or frame timing claim.",
        crate::world::TERRAIN_GENERATOR_VERSION,
        coast::SEED,
        scene.near_triangles,
        scene.near_nodes,
        scene.tiles,
        scene.pages,
        scene.lod_triangles,
        coast::RAYS,
        coast::SEEDS
    );
}
