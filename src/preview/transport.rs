//! Headless admission is completed before choosing the material lighting path.
use crate::render::{post::PostProcess, trace::dynamic::DynamicTargets};
use std::{
    error::Error,
    time::{Duration, Instant},
};

pub(super) mod profile;

/// Explicit controls for convergence and pending-work attribution in static captures.
pub(super) struct Capture {
    pub samples: u32,
    inflight: u32,
}

impl Capture {
    pub fn new(default_samples: u32, tracing: bool, motion: bool) -> Result<Self, Box<dyn Error>> {
        let setting = |name: &str, maximum: u32| -> Result<Option<u32>, Box<dyn Error>> {
            let Some(value) = std::env::var_os(name) else {
                return Ok(None);
            };
            let parsed = value.to_str().and_then(|s| s.parse::<u32>().ok());
            match parsed {
                Some(n) if (1..=maximum).contains(&n) => Ok(Some(n)),
                _ => Err(format!("{name} must be an integer from 1 to {maximum}").into()),
            }
        };
        Ok(Self {
            samples: if motion {
                default_samples
            } else {
                setting("BLOXGLOOM_PREVIEW_SAMPLES", 256)?.unwrap_or(default_samples)
            },
            inflight: if tracing {
                setting("BLOXGLOOM_PREVIEW_GI_INFLIGHT", 32)?.unwrap_or(0)
            } else {
                0
            },
        })
    }

    pub fn checkpoint(
        &self,
        device: &wgpu::Device,
        submission: wgpu::SubmissionIndex,
        sample: u32,
        submit_time: Duration,
    ) -> Result<(), Box<dyn Error>> {
        if self.inflight == 0 || !sample.is_multiple_of(self.inflight) {
            return Ok(());
        }
        let start = Instant::now();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(30)),
            })
            .map_err(|error| format!("GI preview sample {sample} completion failed: {error}"))?;
        println!(
            "GI preview checkpoint: sample={sample}/{} inflight={} submit={:.3}ms completion={:.3}ms",
            self.samples,
            self.inflight,
            submit_time.as_secs_f64() * 1_000.0,
            start.elapsed().as_secs_f64() * 1_000.0,
        );
        Ok(())
    }
}

pub(super) fn prepare(
    post: &mut PostProcess,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    material: &wgpu::RenderPipeline,
    targets: &DynamicTargets,
    eye_in_water: bool,
) -> Result<(), Box<dyn Error>> {
    post.trace.set_eye_water(eye_in_water);
    post.trace
        .prepare_gpu(device, material, post.scene.texture().size());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !post.trace.prepare_dynamic(device, queue, targets) {
        if Instant::now() >= deadline {
            return Err(
                "headless dynamic ray targets could not be admitted within 30 seconds".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let parse = |name: &str| -> Result<Option<u32>, Box<dyn Error>> {
        std::env::var_os(name)
            .map(|value| {
                value
                    .to_str()
                    .and_then(|value| value.parse::<u32>().ok())
                    .ok_or_else(|| format!("{name} must be an integer").into())
            })
            .transpose()
    };
    let edge = parse("BLOXGLOOM_PREVIEW_GI_TILE_EDGE")?;
    let inflight = parse("BLOXGLOOM_PREVIEW_GI_TILE_INFLIGHT")?.unwrap_or(0);
    if edge.is_some() || inflight > 0 {
        post.trace
            .set_headless_transport_scheduling(edge, inflight)?;
        println!(
            "GI headless tile policy: edge={edge:?} inflight={inflight}; checkpoints preserve transport, timestamps include idle spans"
        );
    }
    Ok(())
}
