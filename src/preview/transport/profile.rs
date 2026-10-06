//! Explicit stage timestamps for the actual captured scene, outside frame timing.
use crate::render::post::PostProcess;
use std::error::Error;

fn requested() -> bool {
    std::env::var("BLOXGLOOM_GI_PROFILE").as_deref() == Ok("1")
}

pub(crate) fn features(adapter: &wgpu::Adapter) -> Result<wgpu::Features, Box<dyn Error>> {
    if !requested() {
        return Ok(wgpu::Features::empty());
    }
    if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        return Err("GI preview profiling requires adapter timestamp queries".into());
    }
    Ok(wgpu::Features::TIMESTAMP_QUERY)
}

pub(crate) fn prepare(
    post: &mut PostProcess,
    device: &wgpu::Device,
    samples: u32,
) -> Result<(), Box<dyn Error>> {
    if requested() {
        post.trace.profile(device, samples as usize)?;
        post.trace.profile_active(true);
    }
    Ok(())
}

pub(crate) fn report(
    post: &PostProcess,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), Box<dyn Error>> {
    let Some(stages) = post.trace.profile_results(device, queue)? else {
        return Ok(());
    };
    for (name, mut samples) in ["deformation", "transport", "filter", "composite"]
        .into_iter()
        .zip(stages)
    {
        if samples.is_empty() {
            continue;
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "GI preview GPU {name}: samples={} min={:.3}ms median={:.3}ms max={:.3}ms",
            samples.len(),
            samples[0],
            samples[samples.len() / 2],
            samples[samples.len() - 1],
        );
    }
    println!(
        "GI preview transport timestamps span all tile submissions, including scheduling gaps; profiling readback is outside captured frame timing"
    );
    Ok(())
}
