//! Presentation-side scheduling and revision fencing; geometry stays on the worker.
use super::{State as AudioState, obstruction};
use crate::{
    content::Catalog,
    world::{self, Chunk, ChunkKey},
};
use glam::Vec3;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

const INTERVAL: Duration = Duration::from_millis(100);
const RESULT_TIMEOUT: Duration = Duration::from_millis(200);
const CAPTURE_RADIUS: f32 = obstruction::MAX_DISTANCE + 4.0;

/// Small movement inside the same voxel is safe to accept; crossing a boundary
/// or teleporting requires a new sample instead of applying historical geometry.
pub(super) fn position_matches(current: [f32; 3], sampled: [f32; 3]) -> bool {
    let current = Vec3::from_array(current);
    let sampled = Vec3::from_array(sampled);
    current.is_finite()
        && sampled.is_finite()
        && current.floor() == sampled.floor()
        && current.distance_squared(sampled) <= 0.25
}

struct Capture {
    generation: u64,
    listener: [f32; 3],
    // Retain Arcs, including absent keys, to fence edits, installations and
    // evictions without an allocator pointer-reuse race or deep chunk copies.
    dependencies: Vec<(ChunkKey, Option<Arc<Chunk>>)>,
}
impl Capture {
    fn current(&self, listener: [f32; 3], chunks: &HashMap<ChunkKey, Arc<Chunk>>) -> bool {
        position_matches(listener, self.listener)
            && self
                .dependencies
                .iter()
                .all(|(key, old)| match (old, chunks.get(key)) {
                    (Some(old), Some(new)) => Arc::ptr_eq(old, new),
                    (None, None) => true,
                    _ => false,
                })
    }
}
struct Ready {
    capture: Capture,
    values: Vec<obstruction::Value>,
}
pub(super) struct State {
    worker: Option<obstruction::Worker>,
    generation: u64,
    pending: Option<Capture>,
    ready: Option<Ready>,
    last_submit: Option<Instant>,
    last_sources: Vec<u64>,
}
impl State {
    pub(super) fn new() -> Self {
        #[cfg(not(test))]
        let worker = match obstruction::Worker::spawn() {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "sound obstruction worker unavailable");
                None
            }
        };
        #[cfg(test)]
        let worker = None;
        Self {
            worker,
            generation: 0,
            pending: None,
            ready: None,
            last_submit: None,
            last_sources: Vec::new(),
        }
    }
    #[cfg(test)]
    pub(super) fn enable_for_test(&mut self) {
        self.worker = Some(obstruction::Worker::spawn().unwrap());
    }
    pub(super) fn enabled(&self) -> bool {
        self.worker.is_some()
    }
    pub(super) fn retire(&mut self) {
        self.pending = None;
        self.ready = None;
        self.last_submit = None;
        self.last_sources.clear();
        // Generation is never reset. A result from the previous session cannot
        // match the next submission, even when its terrain is still resident.
    }
    fn accept(&mut self, result: obstruction::ResultBatch) {
        if self.pending.as_ref().is_some_and(|capture| {
            capture.generation == result.generation && capture.listener == result.listener
        }) {
            self.ready = Some(Ready {
                capture: self.pending.take().unwrap(),
                values: result.values,
            });
        }
    }
}
impl AudioState {
    pub(in crate::client) fn poll_obstruction(
        &mut self,
        listener: [f32; 3],
        chunks: &HashMap<ChunkKey, Arc<Chunk>>,
        catalog: &Arc<Catalog>,
        now: Instant,
    ) {
        if !self.obstruction.enabled() {
            return;
        }
        // Sample the last listener accepted by the native queue, matching the
        // accepted positions used by entity-following voices.
        let listener = self
            .last_listener
            .map_or(listener, |(position, _)| position);
        while let Some(result) = self.obstruction.worker.as_ref().and_then(|w| w.poll()) {
            self.obstruction.accept(result);
        }
        if self.obstruction.pending.is_some()
            && self
                .obstruction
                .last_submit
                .is_some_and(|last| now.saturating_duration_since(last) >= RESULT_TIMEOUT)
        {
            // A full result slot across session retirement can discard a newer
            // result. Retry with a fresh generation instead of wedging sampling.
            self.obstruction.pending = None;
            self.obstruction.last_submit = None;
            for source in self.obstruction_sources() {
                self.apply_obstruction(source.id, source.position, 0.35, 2_400.0, now);
            }
        }
        self.apply_ready_obstruction(listener, chunks, now);
        self.start_overdue_obstruction(now);
        let all_sources = self.obstruction_sources();
        let sources: Vec<_> = all_sources
            .iter()
            .copied()
            .filter(|source| {
                Vec3::from_array(source.position).distance(Vec3::from_array(listener))
                    <= obstruction::MAX_DISTANCE
            })
            .collect();
        let ids: Vec<_> = all_sources.iter().map(|s| s.id).collect();
        let due = self
            .obstruction
            .last_submit
            .is_none_or(|last| now.saturating_duration_since(last) >= INTERVAL)
            || ids != self.obstruction.last_sources;
        if !due || self.obstruction.pending.is_some() {
            return;
        }
        for source in &all_sources {
            if Vec3::from_array(source.position).distance(Vec3::from_array(listener))
                > obstruction::MAX_DISTANCE
            {
                self.apply_obstruction(source.id, source.position, 0.35, 2_400.0, now);
            }
        }
        if sources.is_empty() {
            self.obstruction.last_submit = Some(now);
            self.obstruction.last_sources = ids;
            return;
        }
        let Some(generation) = self.obstruction.generation.checked_add(1) else {
            return;
        };
        if !Vec3::from_array(listener).is_finite()
            || listener.iter().any(|n| n.abs() > 16_000_000.0)
        {
            return;
        }
        let min = (Vec3::from_array(listener) - Vec3::splat(CAPTURE_RADIUS))
            .floor()
            .as_ivec3();
        let max = (Vec3::from_array(listener) + Vec3::splat(CAPTURE_RADIUS))
            .floor()
            .as_ivec3();
        let (min, _) = world::world_to_chunk(min.x, min.y, min.z);
        let (max, _) = world::world_to_chunk(max.x, max.y, max.z);
        let mut captured = HashMap::new();
        let mut dependencies = Vec::new();
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let key = ChunkKey { x, y, z };
                    let chunk = chunks.get(&key).cloned();
                    if let Some(chunk) = &chunk {
                        captured.insert(key, Arc::clone(chunk));
                    }
                    dependencies.push((key, chunk));
                }
            }
        }
        let job = obstruction::Job {
            generation,
            listener,
            sources,
            catalog: Arc::clone(catalog),
            chunks: captured,
        };
        if self
            .obstruction
            .worker
            .as_ref()
            .is_some_and(|worker| worker.try_submit(job))
        {
            self.obstruction.generation = generation;
            self.obstruction.last_submit = Some(now);
            self.obstruction.last_sources = ids;
            self.obstruction.pending = Some(Capture {
                generation,
                listener,
                dependencies,
            });
        }
    }
    fn apply_ready_obstruction(
        &mut self,
        listener: [f32; 3],
        chunks: &HashMap<ChunkKey, Arc<Chunk>>,
        now: Instant,
    ) {
        let Some(mut ready) = self.obstruction.ready.take() else {
            return;
        };
        if !ready.capture.current(listener, chunks) {
            self.obstruction.last_submit = None;
            return;
        }
        let sources = self.obstruction_sources();
        ready.values.retain(|value| {
            sources.iter().any(|source| {
                source.id == value.id && position_matches(source.position, value.position)
            }) && !self.apply_obstruction(
                value.id,
                value.position,
                value.gain,
                value.lowpass_hz,
                now,
            )
        });
        if !ready.values.is_empty() {
            self.obstruction.ready = Some(ready);
        }
    }
}

#[cfg(test)]
mod tests;
