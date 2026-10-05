//! Local loading diagnostics and bounded CPU-lane policy. No wire timestamps.
use std::time::{Duration, Instant};

pub(crate) fn worker_count(variable: &str) -> usize {
    let automatic = workers_for(std::thread::available_parallelism().map_or(1, usize::from));
    match std::env::var(variable) {
        Ok(value) => match value.parse::<usize>() {
            Ok(count @ 1..=4) => count,
            _ => {
                tracing::warn!(target: "bloxgloom::lod_loading", %variable, %value, automatic, "invalid LOD worker override; using automatic count");
                automatic
            }
        },
        Err(_) => automatic,
    }
}

fn workers_for(cores: usize) -> usize {
    // Two equally sized pools can be busy in an integrated client/server.
    // Reserve capacity for simulation, near chunks, input and rendering.
    (cores.saturating_sub(2) / 2).clamp(1, 4)
}

pub(crate) fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ClientTrace {
    pub request: u64,
    pub requested: Instant,
    pub received: Instant,
    pub queued: Instant,
}

#[cfg(test)]
mod tests;
