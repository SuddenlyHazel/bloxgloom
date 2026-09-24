//! Frozen-input CPU comparison through the production fire handlers.
//!
//! The measured source phase includes the same post-WAL owner-worker apply
//! function used by production receipts. Disk sync and fixture replanting
//! are outside the CPU gate; delivery has no voxel-owner apply.

use super::apply::FireApplyTimings;
use super::frontier::FireFrontier;
use super::pending::{FireIgnition, FireIgnitionId, FirePending};
use super::scheduler::{
    FireRecovered, FireRuntime, FireWave, FireWaveTimings, frontier_key, mailbox_key,
};
use crate::server::builtins::builtin_phase_plan;
use crate::server::registry::PhasePlan;
use crate::server::simulation::TickId;
use crate::world::{ChunkKey, WOOD, World};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ACTIVE_CHUNKS: usize = 128;
const CHUNKS_X: i32 = 16;
const CHUNKS_Z: i32 = 8;
const FOREST_SEED: u64 = 0x0F1A_4EC0;
static NEXT_BENCH_DIR: AtomicU64 = AtomicU64::new(1);

/// Summed timings and work counters across identical frozen owner waves.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::server) struct FireCpuPhaseReport {
    pub(in crate::server) waves: usize,
    pub(in crate::server) owner_jobs: usize,
    pub(in crate::server) burned_cells: usize,
    pub(in crate::server) effects: usize,
    pub(in crate::server) deferred_owners: usize,
    pub(in crate::server) missing_chunks: usize,
    pub(in crate::server) capture: Duration,
    pub(in crate::server) worker_barrier: Duration,
    pub(in crate::server) validate: Duration,
    pub(in crate::server) route_and_encode: Duration,
}

impl FireCpuPhaseReport {
    pub(in crate::server) fn total(self) -> Duration {
        self.capture + self.worker_barrier + self.validate + self.route_and_encode
    }

    fn record(&mut self, wave: &FireWave, is_source: bool) {
        self.waves += 1;
        self.owner_jobs += wave.transactions.len();
        self.deferred_owners += wave.deferred_owners;
        self.missing_chunks += wave.missing_chunks.len();
        for transaction in &wave.transactions {
            self.burned_cells += transaction.burns.len();
            self.effects += if is_source {
                transaction.emitted_effects
            } else {
                transaction.delivered_effects
            };
        }
        let FireWaveTimings {
            capture,
            worker_barrier,
            validate,
            route_and_encode,
        } = wave.timings;
        self.capture += capture;
        self.worker_barrier += worker_barrier;
        self.validate += validate;
        self.route_and_encode += route_and_encode;
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::server) struct FireCpuRunReport {
    pub(in crate::server) workers: usize,
    pub(in crate::server) source: FireCpuPhaseReport,
    pub(in crate::server) delivery: FireCpuPhaseReport,
    pub(in crate::server) hot_source: FireCpuPhaseReport,
    pub(in crate::server) post_wal_apply: FireApplyTimings,
    pub(in crate::server) output_hash: u64,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::server) struct FireCpuReport {
    pub(in crate::server) active_chunks: usize,
    pub(in crate::server) iterations: usize,
    pub(in crate::server) single_worker: FireCpuRunReport,
    pub(in crate::server) comparison: FireCpuRunReport,
    pub(in crate::server) outputs_match: bool,
    /// True only when the measured source phase used production owner workers.
    pub(in crate::server) post_wal_apply_included: bool,
}

/// Measure one versus `comparison_workers` against independently constructed
/// but identical 128-chunk forests. Each iteration executes the production
/// source/delivery handlers and WAL encoding, then the source wave's actual
/// post-receipt owner-worker installation. Replanting burned cells is outside
/// the measured phases. The benchmark does not submit or wait for a WAL sync.
pub(in crate::server) fn benchmark_cpu(
    iterations: usize,
    comparison_workers: usize,
) -> io::Result<FireCpuReport> {
    if iterations == 0 || comparison_workers == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fire CPU benchmark needs positive iterations and workers",
        ));
    }
    let (mut single_world, source_recovered, delivery_recovered, hot_recovered, single_save) =
        forest_fixture()?;
    let (mut comparison_world, _, _, _, comparison_save) = forest_fixture()?;
    let plan = builtin_phase_plan()?;
    let mut single = Run::new(
        source_recovered.clone(),
        delivery_recovered.clone(),
        hot_recovered.clone(),
        1,
    )?;
    let mut comparison = Run::new(
        source_recovered,
        delivery_recovered,
        hot_recovered,
        comparison_workers,
    )?;

    // JIT is not involved, but a short warmup touches worker threads and the
    // codec caches without contributing fixture setup to measured totals.
    for warmup_tick in 2..=3 {
        let tick = TickId::new(warmup_tick);
        single.prepare(&mut single_world, &plan, tick, false)?;
        comparison.prepare(&mut comparison_world, &plan, tick, false)?;
    }
    for iteration in 0..iterations {
        let number = u64::try_from(iteration)
            .ok()
            .and_then(|iteration| iteration.checked_add(4))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "too many fire iterations")
            })?;
        let tick = TickId::new(number);
        // Alternate order to reduce CPU-cache or thermal advantage to one
        // worker setting, while feeding the same logical tick to both.
        if iteration & 1 == 0 {
            single.prepare(&mut single_world, &plan, tick, true)?;
            comparison.prepare(&mut comparison_world, &plan, tick, true)?;
        } else {
            comparison.prepare(&mut comparison_world, &plan, tick, true)?;
            single.prepare(&mut single_world, &plan, tick, true)?;
        }
    }
    let single_worker = single.into_report();
    let comparison = comparison.into_report();
    drop(single_world);
    drop(comparison_world);
    drop(single_save);
    drop(comparison_save);
    Ok(FireCpuReport {
        active_chunks: ACTIVE_CHUNKS,
        iterations,
        outputs_match: single_worker.output_hash == comparison.output_hash,
        single_worker,
        comparison,
        post_wal_apply_included: true,
    })
}

struct Run {
    workers: usize,
    source: FireRuntime,
    delivery: FireRuntime,
    hot_source: FireRuntime,
    source_report: FireCpuPhaseReport,
    delivery_report: FireCpuPhaseReport,
    hot_report: FireCpuPhaseReport,
    apply_report: FireApplyTimings,
    hash: StableHash,
}

impl Run {
    fn new(
        source: FireRecovered,
        delivery: FireRecovered,
        hot_source: FireRecovered,
        workers: usize,
    ) -> io::Result<Self> {
        Ok(Self {
            workers,
            source: FireRuntime::new(source, workers)?,
            delivery: FireRuntime::new(delivery, workers)?,
            hot_source: FireRuntime::new(hot_source, workers)?,
            source_report: FireCpuPhaseReport::default(),
            delivery_report: FireCpuPhaseReport::default(),
            hot_report: FireCpuPhaseReport::default(),
            apply_report: FireApplyTimings::default(),
            hash: StableHash::new(),
        })
    }

    fn prepare(
        &mut self,
        world: &mut World,
        plan: &PhasePlan,
        tick: TickId,
        measured: bool,
    ) -> io::Result<()> {
        let mut source = self.source.prepare_source_wave(world, plan, tick)?;
        let delivery = self.delivery.prepare_delivery_wave(world, plan, tick)?;
        let hot_source = self.hot_source.prepare_source_wave(world, plan, tick)?;
        if measured {
            self.source_report.record(&source, true);
            self.delivery_report.record(&delivery, false);
            self.hot_report.record(&hot_source, true);
            self.hash.wave(0, tick, &source);
            self.hash.wave(1, tick, &delivery);
            self.hash.wave(2, tick, &hot_source);
        }
        let edits = source
            .transactions
            .iter_mut()
            .filter_map(|transaction| transaction.world_edit.take())
            .collect();
        let apply = self.source.apply_synced_world_edits(world, edits)?;
        if measured {
            self.apply_report.capture_and_validate += apply.capture_and_validate;
            self.apply_report.worker_barrier += apply.worker_barrier;
            self.apply_report.metadata_finalize += apply.metadata_finalize;
            self.apply_report.worker_run_time += apply.worker_run_time;
            self.hash.applied_world(world, &source)?;
        }
        // The next matched input has the same forest blocks but a newer
        // revision. Both worker settings apply and replant exactly once per
        // iteration; this fixture work is deliberately excluded from timing.
        let replant: Vec<_> = source
            .transactions
            .iter()
            .flat_map(|transaction| &transaction.changed_cells)
            .map(|cell| (cell.x, cell.y, cell.z, WOOD))
            .collect();
        if !replant.is_empty() {
            let prepared = world.prepare_edits(&replant)?;
            world.apply_prepared_edits(prepared)?;
        }
        Ok(())
    }

    fn into_report(self) -> FireCpuRunReport {
        FireCpuRunReport {
            workers: self.workers,
            source: self.source_report,
            delivery: self.delivery_report,
            hot_source: self.hot_report,
            post_wal_apply: self.apply_report,
            output_hash: self.hash.value,
        }
    }
}

fn forest_fixture() -> io::Result<(
    World,
    FireRecovered,
    FireRecovered,
    FireRecovered,
    BenchSave,
)> {
    let save = BenchSave::new()?;
    let mut world = World::with_capacity(FOREST_SEED, save.path.clone(), ACTIVE_CHUNKS + 16)?;
    let mut source = FireRecovered::default();
    let mut delivery = FireRecovered::default();
    let mut hot_source = FireRecovered::default();
    let mut edits = Vec::with_capacity(ACTIVE_CHUNKS * 32);
    for z in 0..CHUNKS_Z {
        for x in 0..CHUNKS_X {
            let owner = ChunkKey { x, y: 4, z };
            let (epoch, _) = world.begin_chunk_load(owner)?;
            let loaded = world.load_chunk_uncached(owner)?;
            if !world.install_loaded_if_absent(loaded, epoch)? {
                return Err(io::Error::other(
                    "fire benchmark failed to install forest chunk",
                ));
            }
            let mut frontier = FireFrontier::default();
            let mut mailbox = FirePending::default();
            for local_y in 1..=2 {
                for local_z in [0, 1, 14, 15] {
                    for local_x in [0, 1, 14, 15] {
                        let cell = cell_index(local_x, local_y, local_z);
                        edits.push((x * 16 + local_x, 64 + local_y, z * 16 + local_z, WOOD));
                        frontier.insert(cell, 1)?;
                        if local_x == 0 {
                            mailbox.insert(FireIgnition {
                                id: FireIgnitionId {
                                    source_tick: 1,
                                    source_cell: cell_index(15, local_y, local_z),
                                    direction: 1,
                                },
                                target_cell: cell,
                                activate_at: 2,
                            })?;
                        }
                    }
                }
            }
            source.apply_value(&frontier_key(owner), &frontier.encode())?;
            if x == 0 && z == 0 {
                hot_source.apply_value(&frontier_key(owner), &frontier.encode())?;
            }
            let western_source = ChunkKey { x: x - 1, ..owner };
            delivery.apply_value(&mailbox_key(owner, western_source), &mailbox.encode())?;
        }
    }
    let prepared = world.prepare_edits(&edits)?;
    world.apply_prepared_edits(prepared)?;
    if world.resident_chunk_count() != ACTIVE_CHUNKS {
        return Err(io::Error::other(
            "fire benchmark forest is not fully resident",
        ));
    }
    Ok((world, source, delivery, hot_source, save))
}

fn cell_index(x: i32, y: i32, z: i32) -> u16 {
    (x + 16 * (z + 16 * y)) as u16
}

struct StableHash {
    value: u64,
}

impl StableHash {
    fn new() -> Self {
        Self {
            value: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.value ^= bytes.len() as u64;
        self.value = self.value.wrapping_mul(0x100_0000_01b3);
        for &byte in bytes {
            self.value ^= u64::from(byte);
            self.value = self.value.wrapping_mul(0x100_0000_01b3);
        }
    }

    fn wave(&mut self, phase: u8, tick: TickId, wave: &FireWave) {
        self.bytes(&[phase]);
        self.bytes(&tick.get().to_le_bytes());
        self.bytes(&(wave.transactions.len() as u64).to_le_bytes());
        for transaction in &wave.transactions {
            let owner = transaction.owner();
            for coordinate in [owner.x, owner.y, owner.z] {
                self.bytes(&coordinate.to_le_bytes());
            }
            self.bytes(&(transaction.changes().len() as u64).to_le_bytes());
            for change in transaction.changes() {
                self.bytes(change.key.domain.as_bytes());
                self.bytes(&change.key.bytes);
                self.bytes(&change.before);
                self.bytes(&change.after);
            }
        }
    }

    fn applied_world(&mut self, world: &mut World, wave: &FireWave) -> io::Result<()> {
        self.bytes(&[3]);
        for transaction in &wave.transactions {
            if transaction.changed_cells.is_empty() {
                continue;
            }
            let owner = transaction.owner();
            let chunk = world
                .cached_arc_chunk(owner)
                .ok_or_else(|| io::Error::other("applied fire owner vanished from cache"))?;
            self.bytes(&chunk.version.to_le_bytes());
            for &cell in &transaction.burns {
                let block = chunk
                    .block_index(usize::from(cell))
                    .ok_or_else(|| io::Error::other("applied fire cell is out of bounds"))?;
                self.bytes(&cell.to_le_bytes());
                self.bytes(&block.0.to_le_bytes());
            }
        }
        Ok(())
    }
}

struct BenchSave {
    path: PathBuf,
}

impl BenchSave {
    fn new() -> io::Result<Self> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| io::Error::other(format!("benchmark clock: {error}")))?
            .as_nanos();
        let sequence = NEXT_BENCH_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-fire-cpu-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for BenchSave {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
