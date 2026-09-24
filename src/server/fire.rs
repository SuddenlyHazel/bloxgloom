//! WAL-backed, chunk-owned fire simulation.
//!
//! The frontier and source-scoped ignition mailboxes are authoritative save
//! data. A worker may prepare their next values, but only a synced WAL receipt
//! installs them or publishes a block edit.

mod apply;
mod bench;
mod checkpoint;
mod codec;
mod frontier;
mod handler;
mod pending;
mod scheduler;

#[cfg(test)]
mod tests;

pub(in crate::server) use bench::benchmark_cpu;
pub(super) use checkpoint::FireCheckpointStore;
use frontier::FireFrontier;
pub(super) use handler::{FireDeliveryHandler, FireHandler};
use pending::{FireIgnition, FireIgnitionId, FirePending};
pub(super) use scheduler::{FireRecovered, FireRuntime, FireSeed, FireTransaction, FireWave};
