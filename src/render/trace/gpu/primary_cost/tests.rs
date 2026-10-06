//! Ignored diagnostics reuse normal raster/frame assembly, never synthetic MRTs.
use super::*;

#[test]
fn actual_primary_counter_source_validates_and_retains_exact_hooks() {
    for (lobes, raw) in [(false, false), (true, false), (true, true)] {
        let base = super::super::shaders::transport_with_lod_vector(lobes, raw, false);
        for instrumented in [false, true] {
            let source = shader::source(base.clone(), instrumented);
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
}

fn controls() {
    assert_eq!(std::env::var("BLOXGLOOM_GI").as_deref(), Ok("1"));
    assert!(!crate::render::bsl_reference::enabled());
    for flag in [
        "BLOXGLOOM_GI_PROFILE",
        "BLOXGLOOM_GI_DIAGNOSTICS",
        "BLOXGLOOM_GI_LOD_VECTOR_LOAD",
    ] {
        assert_ne!(
            std::env::var(flag).as_deref(),
            Ok("1"),
            "sparse scalar attribution bypasses normal transport/profiling; disable {flag}"
        );
    }
}

fn report(state: State, label: &str) {
    assert!(state.captured, "production frame did not reach the probe");
    assert_eq!(state.counts.len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(state.classes.len(), state.counts.len());
    let mut totals = [0_u64; 15];
    let mut maxima = [0_u32; 15];
    // Complete water, other water, actor, medium-only, unknown surface, opaque.
    let mut families = [[0_u64; 16]; 6];
    for (counts, [class, age]) in state.counts.iter().zip(&state.classes) {
        let family = if *age == -2.0 {
            2
        } else if *class < -2.0 {
            0
        } else if *class < 0.0 {
            1
        } else if *class > 1.5 {
            3
        } else if *age == -1.0 {
            4
        } else {
            5
        };
        families[family][0] += 1;
        for (index, count) in counts.iter().enumerate() {
            totals[index] += u64::from(*count);
            maxima[index] = maxima[index].max(*count);
            families[family][index + 1] += u64::from(*count);
        }
    }
    assert!(totals[0] > 0 && totals[1] > 0 && totals[4] > 0);
    println!(
        "ACTUAL PRIMARY {label}: 16 real 2x2 low-resolution quads; exact original/instrumented production-format stored HDR, geometry, transmission and current-correction, with unquantized exact RNG halves/class/age controls. Raw FP32 deltas are separately exported and reported; no unquantized HDR bit-identity claim. Counter order: BVH nodes, triangle tests, known DDA, cloud delta proposals, path vertices, medium region calls, directory iterations, cloud density calls (including shadow quadrature), material surface calls, accepted alpha evaluations, primary lighting calls, camera-medium calls, primary water split calls, guide geometry queries, dynamic static replays. totals={totals:?}; maxima={maxima:?}; family rows [sample count,15 totals] (completeWater,otherWater,actor,mediaOnly,unknown,opaque)={families:?}. Original raster MRT/depth, authoritative near+selected LOD scene and material/dynamic bindings are reused. These sparse first-frame counts are NOT full-frame timing, history/convergence or visual acceptance. Rendered images and benchmark timings from this diagnostic scope are intentionally not GI results."
    );
}

#[test]
#[ignore = "BLOXGLOOM_GI=1 BLOXGLOOM_LANDSCAPE_VIEW=coast; exclusive GPU, actual raster primary attribution only"]
fn gpu_actual_coast_primary_query_counts_preserve_hdr_rng_and_classification() {
    controls();
    assert_eq!(
        std::env::var("BLOXGLOOM_LANDSCAPE_VIEW").as_deref(),
        Ok("coast")
    );
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "bloxgloom-primary-cost-coast-{}-{unique}",
        std::process::id()
    ));
    let scope = Scope::enter();
    crate::preview::render_landscape_previews(&directory).unwrap();
    report(scope.finish(), "generated 512m coast");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "BLOXGLOOM_GI=1 BLOXGLOOM_PERF_PRELOAD=1; exclusive GPU, actual raster primary attribution only"]
fn gpu_actual_origin_primary_query_counts_preserve_hdr_rng_and_classification() {
    controls();
    assert_eq!(std::env::var("BLOXGLOOM_PERF_PRELOAD").as_deref(), Ok("1"));
    let scope = Scope::enter();
    crate::preview::run_lod_benchmark(1, 6, 512, false).unwrap();
    report(scope.finish(), "preloaded radius6/512m origin");
}
