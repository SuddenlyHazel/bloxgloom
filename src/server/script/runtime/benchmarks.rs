//! Explicit opt-in measurement; timings are evidence, never pass/fail thresholds.
use super::super::{Execution, create};
use std::time::Instant;

const SOURCE: &str = r#"
local evaluations = 1
local lookup = {}
for i=1,128 do lookup[i] = i * i end
return function(tick)
    local total = 0
    for i=1,128 do total += lookup[i] end
    return total + tick, evaluations
end
"#;

fn report(name: &str, mut ns: Vec<u128>, evaluations: usize, bytes: Option<usize>) {
    ns.sort_unstable();
    let percentile = |percent: usize| ns[(ns.len() - 1) * percent / 100] as f64 / 1_000.0;
    let memory = bytes.map_or_else(|| "not_sampled".into(), |bytes| bytes.to_string());
    eprintln!(
        "{name}: samples={} p50={:.2}us p95={:.2}us p99={:.2}us module_evaluations={evaluations} post_gc_bytes={memory}",
        ns.len(),
        percentile(50),
        percentile(95),
        percentile(99)
    );
}

#[test]
#[ignore = "opt-in VM lifetime latency measurement; run alone with --nocapture"]
fn vm_lifetime_latency_baseline() {
    const SAMPLES: usize = 2_000;
    let mut cold = Vec::with_capacity(SAMPLES);
    let mut bytes = 0;
    for tick in 0..SAMPLES {
        let start = Instant::now();
        let (lua, diagnostics) = create(
            "benchmark:callback",
            Execution::new("benchmark", tick as u64, "baseline"),
        )
        .unwrap();
        let callback: mlua::Function = lua
            .load(SOURCE)
            .set_name("benchmark:callback")
            .eval()
            .unwrap();
        let result: (u64, u64) = callback.call(tick).unwrap();
        assert_eq!(result, (707264 + tick as u64, 1));
        diagnostics.finish("evaluated");
        cold.push(start.elapsed().as_nanos());
        if tick + 1 == SAMPLES {
            drop(callback);
            lua.gc_collect().unwrap();
            bytes = lua.used_memory();
        }
    }
    report("fresh_vm_compile_callback", cold, SAMPLES, Some(bytes));

    let (lua, diagnostics) = create(
        "benchmark:callback",
        Execution::new("benchmark", 0, "baseline"),
    )
    .unwrap();
    let mut reevaluated = Vec::with_capacity(SAMPLES);
    for tick in 0..SAMPLES {
        let start = Instant::now();
        let callback: mlua::Function = lua
            .load(SOURCE)
            .set_name("benchmark:callback")
            .eval()
            .unwrap();
        let result: (u64, u64) = callback.call(tick).unwrap();
        assert_eq!(result, (707264 + tick as u64, 1));
        reevaluated.push(start.elapsed().as_nanos());
    }
    lua.gc_collect().unwrap();
    report(
        "reused_vm_compile_callback",
        reevaluated,
        SAMPLES,
        Some(lua.used_memory()),
    );

    let callback: mlua::Function = lua
        .load(SOURCE)
        .set_name("benchmark:callback")
        .eval()
        .unwrap();
    let mut retained = Vec::with_capacity(SAMPLES);
    for tick in 0..SAMPLES {
        let start = Instant::now();
        let result: (u64, u64) = callback.call(tick).unwrap();
        assert_eq!(result, (707264 + tick as u64, 1));
        retained.push(start.elapsed().as_nanos());
    }
    lua.gc_collect().unwrap();
    report(
        "retained_module_callback",
        retained,
        1,
        Some(lua.used_memory()),
    );
    diagnostics.finish("evaluated");
}

#[test]
#[ignore = "opt-in production runner comparison; run alone with --nocapture"]
fn vm_lifetime_production_runner_latency() {
    use super::super::{Retained, isolated};
    use crate::server::script::{Limits, Program, SourceModule};
    const SAMPLES: usize = 2_000;
    let limits = Limits::default();
    let module = SourceModule {
        id: "benchmark:callback".into(),
        source: SOURCE.into(),
    };
    let program = Program::Source(SourceModule {
        id: module.id.clone(),
        source: module.source.clone(),
    });
    let mut isolated_ns = Vec::with_capacity(SAMPLES);
    let mut isolated_heap = 0;
    for tick in 0..SAMPLES {
        let start = Instant::now();
        let result: (u64, u64) = isolated(
            &program,
            limits,
            Execution::new("benchmark", tick as u64, "production"),
            |lua, entry| {
                let result = entry.call(tick)?;
                isolated_heap = isolated_heap.max(lua.used_memory());
                Ok(result)
            },
        )
        .unwrap();
        assert_eq!(result, (707264 + tick as u64, 1));
        isolated_ns.push(start.elapsed().as_nanos());
    }
    eprintln!(
        "production_isolated_attempt: cold={:.2}us sampled_heap_peak_before_cleanup={isolated_heap}",
        isolated_ns[0] as f64 / 1_000.0
    );
    isolated_ns.remove(0);
    report(
        "production_isolated_warm_attempt",
        isolated_ns,
        SAMPLES,
        None,
    );

    let mut realm = Retained::default();
    let mut retained_ns = Vec::with_capacity(SAMPLES);
    let mut retained_heap = 0;
    for tick in 0..SAMPLES {
        let start = Instant::now();
        let result: (u64, u64) = realm
            .run_source(
                &module,
                limits,
                Execution::new("benchmark", tick as u64, "production").client(),
                |lua, entry| {
                    let result = entry.call(tick)?;
                    retained_heap = retained_heap.max(lua.used_memory());
                    Ok(result)
                },
            )
            .unwrap();
        assert_eq!(result, (707264 + tick as u64, 1));
        retained_ns.push(start.elapsed().as_nanos());
    }
    eprintln!(
        "production_retained_callback: cold={:.2}us sampled_heap_peak_before_cleanup={retained_heap}",
        retained_ns[0] as f64 / 1_000.0
    );
    retained_ns.remove(0);
    report("production_retained_warm_callback", retained_ns, 1, None);

    let unrelated = Program::Source(SourceModule {
        id: "benchmark:unrelated".into(),
        source: "return function() local total=0; for i=1,1000 do total+=i end; return total end"
            .into(),
    });
    let mut mixed = Vec::with_capacity(SAMPLES);
    for tick in 0..SAMPLES {
        let start = Instant::now();
        let total: u64 = isolated(
            &unrelated,
            limits,
            Execution::new("benchmark", tick as u64, "mixed"),
            |_, entry| entry.call(()),
        )
        .unwrap();
        assert_eq!(total, 500_500);
        let result: (u64, u64) = isolated(
            &program,
            limits,
            Execution::new("benchmark", tick as u64, "mixed"),
            |_, entry| entry.call(tick),
        )
        .unwrap();
        assert_eq!(result, (707264 + tick as u64, 1));
        mixed.push(start.elapsed().as_nanos());
    }
    report("production_mixed_two_attempts", mixed, SAMPLES * 2, None);
}
