//! Bounded acoustic transmission through captured, authoritative voxel chunks.
//!
//! This is obstruction plus a small doorway/edge approximation, not wave
//! simulation. A single bend may travel up to three metres around the first
//! barrier; it cannot infer openings in missing chunks. Only endpoint voxels
//! are omitted, so sounds anchored inside a machine do not muffle themselves.
use crate::{
    content::Catalog,
    world::{self, Chunk, ChunkKey},
};
use glam::{IVec3, Vec3};
use std::{
    collections::HashMap,
    io,
    sync::{Arc, mpsc},
    thread,
};

pub(in crate::client) const MAX_DISTANCE: f32 = 32.0;
pub(in crate::client) const MAX_CHUNKS: usize = 512;
pub(in crate::client) const MAX_SOURCES: usize = 32;
const MAX_RAY_CELLS: usize = 128;
const MIN_GAIN: f32 = 0.025;

#[derive(Clone, Copy, Debug)]
pub(in crate::client) struct Source {
    pub id: u64,
    pub position: [f32; 3],
}
pub(in crate::client) struct Job {
    pub generation: u64,
    pub listener: [f32; 3],
    pub sources: Vec<Source>,
    pub catalog: Arc<Catalog>,
    pub chunks: HashMap<ChunkKey, Arc<Chunk>>,
}
#[derive(Clone, Copy, Debug)]
pub(in crate::client) struct Value {
    pub id: u64,
    pub position: [f32; 3],
    pub gain: f32,
    pub lowpass_hz: f32,
}
pub(in crate::client) struct ResultBatch {
    pub generation: u64,
    pub listener: [f32; 3],
    pub values: Vec<Value>,
}
pub(in crate::client) struct Worker {
    sender: mpsc::SyncSender<Job>,
    receiver: mpsc::Receiver<ResultBatch>,
}
impl Worker {
    pub fn spawn() -> io::Result<Self> {
        let (sender, jobs) = mpsc::sync_channel::<Job>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("sound-obstruction".into())
            .spawn(move || {
                while let Ok(mut job) = jobs.recv() {
                    // Prefer the newest pending snapshot before starting work.
                    while let Ok(newer) = jobs.try_recv() {
                        job = newer;
                    }
                    let result = solve(&job);
                    // Never block the worker behind an unconsumed result.
                    if results.try_send(result).is_err() {
                        continue;
                    }
                }
            })?;
        Ok(Self { sender, receiver })
    }
    pub fn try_submit(&self, job: Job) -> bool {
        valid(&job) && self.sender.try_send(job).is_ok()
    }
    pub fn poll(&self) -> Option<ResultBatch> {
        self.receiver.try_recv().ok()
    }
}
fn valid(job: &Job) -> bool {
    let valid_position = |p: [f32; 3]| p.iter().all(|n| n.is_finite() && n.abs() <= 16_000_000.0);
    valid_position(job.listener)
        && job.sources.len() <= MAX_SOURCES
        && job.chunks.len() <= MAX_CHUNKS
        && job.sources.iter().all(|s| {
            valid_position(s.position)
                && Vec3::from_array(s.position).distance(Vec3::from_array(job.listener))
                    <= MAX_DISTANCE
        })
        && job.chunks.iter().all(|(key, chunk)| *key == chunk.key)
}
fn block(job: &Job, p: IVec3) -> Option<world::BlockId> {
    let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
    job.chunks.get(&key)?.block(local)
}
fn transmission(job: &Job, p: IVec3) -> f32 {
    let Some(id) = block(job, p) else {
        return 0.08;
    };
    let Some(def) = job.catalog.block(id) else {
        return 0.08;
    };
    if !def.solid {
        return 1.0;
    }
    if def.cutout {
        return 0.94;
    }
    use bloxgloom_host_api::content::RainSurface;
    match job.catalog.block_acoustics(id).map(|a| a.surface) {
        Some(RainSurface::Wood) => 0.45,
        Some(RainSurface::Leaf) => 0.94,
        Some(RainSurface::Glass) => 0.4,
        Some(RainSurface::Dirt) => 0.3,
        _ if def.key == "bloxgloom:wood" => 0.45,
        _ => 0.25,
    }
}
struct Ray {
    gain: f32,
    barrier: Option<IVec3>,
    cells: usize,
}
fn ray(job: &Job, a: Vec3, b: Vec3, endpoints: [IVec3; 2]) -> Ray {
    let delta = b - a;
    let length = delta.length();
    if length < 0.0001 {
        return Ray {
            gain: if block(job, a.floor().as_ivec3())
                .is_some_and(|id| job.catalog.block(id).is_some())
            {
                1.0
            } else {
                MIN_GAIN
            },
            barrier: None,
            cells: 0,
        };
    }
    let mut cell = a.floor().as_ivec3();
    let mut next = [f32::INFINITY; 3];
    let mut step = [0; 3];
    let mut increment = [f32::INFINITY; 3];
    for axis in 0..3 {
        if delta[axis] != 0.0 {
            step[axis] = if delta[axis] > 0.0 { 1 } else { -1 };
            let face = cell[axis] as f32 + if step[axis] > 0 { 1.0 } else { 0.0 };
            next[axis] = ((face - a[axis]) / delta[axis]).max(0.0);
            increment[axis] = 1.0 / delta[axis].abs();
        }
    }
    let mut start = 0.0;
    let mut gain = 1.0;
    let mut barrier = None;
    for count in 0..MAX_RAY_CELLS {
        let end = next.into_iter().fold(1.0, f32::min);
        let known_endpoint = endpoints.contains(&cell)
            && block(job, cell).is_some_and(|id| job.catalog.block(id).is_some());
        if !known_endpoint && end > start {
            let t = transmission(job, cell);
            if t < 0.9 && barrier.is_none() {
                barrier = Some(cell);
            }
            gain *= t.powf((end - start) * length);
        }
        if end >= 1.0 {
            return Ray {
                gain,
                barrier,
                cells: count + 1,
            };
        }
        // Advance every tied axis, avoiding zero-length edge/corner cells.
        for axis in 0..3 {
            if next[axis] <= end + 0.000001 {
                cell[axis] += step[axis];
                next[axis] += increment[axis];
            }
        }
        start = end;
    }
    Ray {
        gain: 0.0,
        barrier,
        cells: MAX_RAY_CELLS,
    }
}
fn obstruction(job: &Job, source: Source) -> (Value, usize) {
    let a = Vec3::from_array(job.listener);
    let b = Vec3::from_array(source.position);
    let endpoints = [a.floor().as_ivec3(), b.floor().as_ivec3()];
    let direct = ray(job, a, b, endpoints);
    let mut gain = direct.gain;
    let mut cells = direct.cells;
    if let Some(barrier) = direct.barrier {
        let direction = (b - a).normalize_or_zero();
        let horizontal = Vec3::new(-direction.z, 0.0, direction.x).normalize_or_zero();
        let horizontal = if horizontal.length_squared() < 0.5 {
            Vec3::X
        } else {
            horizontal
        };
        let vertical = direction.cross(horizontal).normalize_or_zero();
        for offset in [horizontal, -horizontal, vertical, -vertical] {
            for radius in [1.0, 2.0, 3.0] {
                let bend = barrier.as_vec3() + Vec3::splat(0.5) + offset * radius;
                // A solid or unknown waypoint is never an opening.
                if transmission(job, bend.floor().as_ivec3()) < 0.9 {
                    continue;
                }
                let first = ray(job, a, bend, endpoints);
                let second = ray(job, bend, b, endpoints);
                cells += first.cells + second.cells;
                let detour = (a.distance(bend) + bend.distance(b) - a.distance(b)).max(0.0);
                let alternative = first.gain * second.gain * 0.65 / (1.0 + detour * 0.25);
                gain = gain.max(alternative);
            }
        }
    }
    let gain = gain.clamp(MIN_GAIN, 1.0);
    (
        Value {
            id: source.id,
            position: source.position,
            gain,
            lowpass_hz: 700.0 + 19_300.0 * gain * gain,
        },
        cells,
    )
}
fn solve(job: &Job) -> ResultBatch {
    ResultBatch {
        generation: job.generation,
        listener: job.listener,
        values: job
            .sources
            .iter()
            .map(|source| obstruction(job, *source).0)
            .collect(),
    }
}
#[cfg(test)]
mod tests;
