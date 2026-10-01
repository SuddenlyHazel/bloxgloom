//! Server moving-object capture, integration and durable reaction lifecycle.
pub(in crate::server) mod cadence;
pub(in crate::server) mod colliders;
pub(in crate::server) mod services;
pub(in crate::server) mod solver;
mod tick;
pub(in crate::server) use tick::plan;

pub(in crate::server) const STEP_TICKS: u64 = 2;
pub(in crate::server) const DT: f64 = 0.04;
pub(in crate::server) const MAX_BODIES: usize = 256;
pub(in crate::server) const MAX_CHUNK_BODIES: usize = 64;
pub(in crate::server) const MAX_SWEEP_CELLS: usize = 4096;
pub(in crate::server) const MAX_COLLIDERS: usize = 4160;
pub(in crate::server) const MAX_DYNAMIC_COLLIDERS: usize = 64;
