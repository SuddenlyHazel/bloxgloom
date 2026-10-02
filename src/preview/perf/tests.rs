use super::*;

#[test]
fn gpu_terrain_benchmark_times_shadow_pass_without_empty_timestamp_descriptors() {
    // A tiny real run catches timestamp descriptor validation and exercises the
    // same upload/steady path as perf 300 6. No performance threshold in tests.
    pollster::block_on(run_perf_benchmark_async(1, 1, false, 0)).unwrap();
}
