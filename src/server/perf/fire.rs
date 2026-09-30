//! Paced fire workload through production ticks, WAL submission, and receipts.
//!
//! Only the isolated initial forest/frontier is benchmark-directed. During the
//! measured interval the registered fire systems, durable writer, checkpoint
//! worker, and owner apply are exactly the live server path.

use super::fixture::TempSaveDir;
use crate::server::durable::{self, CommitAction, StageError};
use crate::server::metrics::{LatencyEvent, TickSample};
use crate::server::simulation::{FIXED_STEP, TickId};
use crate::server::{State, runtime, server_state};
use crate::world::{ChunkKey, GLOWSTONE, WOOD, World, world_to_chunk};
use std::collections::BTreeMap;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

mod replant;
use replant::Replanter;

const SEED: u64 = 0x0F1A_4EC0;
const CHUNKS_X: i32 = 16;
const CHUNKS_Z: i32 = 8;
const ACTIVE_CHUNKS: usize = (CHUNKS_X * CHUNKS_Z) as usize;
const FOREST_BATCH_CHUNKS: usize = 8;
const MAX_DRAIN_POLLS: usize = 5_000;

pub(super) fn run(measured_ticks: usize, warmup: usize) -> io::Result<()> {
    if !(300..=15_000).contains(&measured_ticks) || warmup > 15_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fire soak needs 300..=15000 measured ticks and <=15000 warmup ticks",
        ));
    }
    let save = TempSaveDir::create()?;
    let mut state = server_state(SEED, save.path.clone())?;
    let owners = forest_owners();
    install_forest(&mut state, &owners)?;
    let initial_wood = forest_wood_count(&mut state.world, &owners)?;

    // The dense frontier is initialized as fully WAL-durable fire state. It
    // is admitted one cursor lane per owner in each wave, then its receipts
    // are applied before the next wave reads cursor before-values. All warmup
    // ticks then run the active production fire workload, not an idle prelude.
    install_initial_frontiers(&mut state, &owners, TickId::new(1))?;
    let initial_frontier = state.fire.load_metrics().frontier_cells;
    let start_fire = state.fire.load_metrics();
    let mut ignition = IgnitionSource::default();
    let mut replanter = Replanter::default();
    let mut deadline = Instant::now() + FIXED_STEP;
    for number in 1..=warmup {
        let tick = TickId::new((number + 1) as u64);
        paced_tick(&mut state, tick, &mut deadline, |state, tick| {
            ignition.attempt(state, tick)?;
            replanter.attempt(state, &owners, tick)
        })?;
    }

    let rotation_before = state.durability.completed_rotations;
    let measured_fire_before = state.fire.load_metrics();
    let measured_wood_before = forest_wood_count(&mut state.world, &owners)?;
    let measured_started = Instant::now();
    deadline = measured_started + FIXED_STEP;
    let mut samples = Vec::with_capacity(measured_ticks);
    let mut preparation_samples = Vec::with_capacity(measured_ticks);
    let mut runtime_samples = Vec::with_capacity(measured_ticks);
    let mut frontier_samples = Vec::with_capacity(measured_ticks);
    let mut pending_samples = Vec::with_capacity(measured_ticks);
    let mut rotation_stall_ticks = 0usize;
    let mut max_checkpoint_backlog = 0usize;
    let mut max_pending_commits = 0usize;
    for offset in 0..measured_ticks {
        let tick = TickId::new((warmup + offset + 2) as u64);
        // Trigger one checkpoint-gated rotation against this isolated save.
        // Production WAL limits remain unchanged.
        if offset == 20 {
            state.durability.force_rotation_at_sequence = Some(state.durability.writer.sequence());
        }
        let (sample, preparation, runtime) =
            paced_tick(&mut state, tick, &mut deadline, |state, tick| {
                ignition.attempt(state, tick)?;
                replanter.attempt(state, &owners, tick)
            })?;
        preparation_samples.push(preparation.as_nanos().min(u64::MAX as u128) as u64);
        runtime_samples.push(runtime.as_nanos().min(u64::MAX as u128) as u64);
        samples.push(sample);
        rotation_stall_ticks += usize::from(state.durability.rotation_requested);
        max_checkpoint_backlog = max_checkpoint_backlog.max(
            state.durability.dirty_checkpoints.len() + state.durability.checkpoint_inflight.len(),
        );
        max_pending_commits = max_pending_commits.max(state.durability.pending.len());
        let load = state.fire.load_metrics();
        frontier_samples.push(load.frontier_cells as u64);
        pending_samples.push(load.pending_ignitions as u64);
    }
    let wall = measured_started.elapsed();
    let measured_fire_after = state.fire.load_metrics();
    let last_tick = TickId::new((warmup + measured_ticks + 2) as u64);
    drain_durable(&mut state, last_tick, true)?;
    let final_fire = state.fire.load_metrics();
    let final_wood = forest_wood_count(&mut state.world, &owners)?;
    let world_hash = forest_hash(&mut state.world, &owners)?;
    let fire_hash = state.fire.benchmark_state_fingerprint();
    let rotations = state.durability.completed_rotations - rotation_before;
    let wal_p95 = state
        .metrics
        .latency_summary(LatencyEvent::DurableWalReceipt)
        .map(|summary| summary.p95);
    let remaining_actions = state.durability.pending.len() + state.durability.queued.len();
    drop(state);

    // Reopen from the same isolated v5 save and compare authoritative values,
    // not just the in-memory chunk cache that processed receipts.
    let mut restarted = server_state(SEED, save.path.clone())?;
    for &owner in &owners {
        ensure_resident(&mut restarted.world, owner)?;
    }
    let restart_world_hash = forest_hash(&mut restarted.world, &owners)?;
    let restart_fire_hash = restarted.fire.benchmark_state_fingerprint();
    let restart_fire = restarted.fire.load_metrics();
    drop(restarted);
    drop(save);

    let tick_times: Vec<_> = samples
        .iter()
        .map(|sample| sample.tick_total.as_nanos().min(u64::MAX as u128) as u64)
        .collect();
    let (_, p50, p95, p99) = percentiles(tick_times);
    let (_, _, preparation_p95, _) = percentiles(preparation_samples);
    let (_, _, runtime_p95, runtime_p99) = percentiles(runtime_samples);
    let (frontier_min, frontier_median, _, frontier_p99) = percentiles(frontier_samples.clone());
    let (_, pending_median, _, pending_p99) = percentiles(pending_samples);
    let in_band = frontier_samples
        .iter()
        .filter(|&&cells| (8_000..=16_000).contains(&cells))
        .count();
    let max_backlog = samples
        .iter()
        .map(|sample| sample.backlog_ticks)
        .max()
        .unwrap_or_default();
    let max_backlog_streak = longest_backlog_streak(&samples);
    let queue_window = (samples.len() / 2).min(1_000);
    let queue_first = mean_pending(&samples[..queue_window]);
    let queue_last = mean_pending(&samples[samples.len() - queue_window..]);
    let admitted =
        measured_fire_after.admitted_transactions - measured_fire_before.admitted_transactions;
    let deferred =
        measured_fire_after.deferred_transactions - measured_fire_before.deferred_transactions;
    let burns = final_fire.burned_cells - start_fire.burned_cells;
    let wood_loss = initial_wood
        .saturating_add(replanter.replanted)
        .saturating_sub(final_wood);
    let wood_budget = burns.saturating_add(ignition.replaced_wood);
    let restart_match = world_hash == restart_world_hash
        && fire_hash == restart_fire_hash
        && final_fire.frontier_cells == restart_fire.frontier_cells
        && final_fire.pending_ignitions == restart_fire.pending_ignitions;
    let conservation = final_wood <= measured_wood_before.saturating_add(replanter.replanted)
        && wood_loss <= wood_budget
        && initial_wood >= final_wood;
    let active_band = in_band == measured_ticks;
    let backlog_ok = max_backlog_streak <= 50 && queue_last <= queue_first + 1.0;
    let passed = restart_match
        && conservation
        && rotations >= 1
        && remaining_actions == 0
        && admitted > 0
        && ignition.accepted > 0
        && burns > 0
        && active_band
        && backlog_ok
        && p95 <= 15_000_000
        && p99 <= 20_000_000;
    println!(
        "fire WAL soak: {measured_ticks} measured ticks, {warmup} warmup ticks, {ACTIVE_CHUNKS} dense forest chunks, 50 Hz paced; wall {:.3}s; harness-inclusive tick CPU ms p50 {:.3} p95 {:.3} p99 {:.3} [benchmark edit prep p95 {:.3}, production runtime p95 {:.3} p99 {:.3}]; WAL receipt p95 {} ms",
        wall.as_secs_f64(),
        p50 as f64 / 1e6,
        p95 as f64 / 1e6,
        p99 as f64 / 1e6,
        preparation_p95 as f64 / 1e6,
        runtime_p95 as f64 / 1e6,
        runtime_p99 as f64 / 1e6,
        wal_p95.map_or_else(
            || "n/a".to_string(),
            |value| format!("{:.3}", value as f64 / 1e6)
        )
    );
    println!(
        "  fire: initial frontier {initial_frontier}, post-warmup frontier {}, measured frontier min/median/p99 {frontier_min}/{frontier_median}/{frontier_p99}, in 8k..16k {in_band}/{measured_ticks}; pending ignition median/p99 {pending_median}/{pending_p99}; measured WAL fire admitted {admitted}, deferred {deferred} [conflict {}, full {}], worker-deferred {}, missing chunks {}; benchmark-directed glowstone edits admitted {}/attempted {}",
        measured_fire_before.frontier_cells,
        measured_fire_after.conflict_deferred_transactions
            - measured_fire_before.conflict_deferred_transactions,
        measured_fire_after.full_deferred_transactions
            - measured_fire_before.full_deferred_transactions,
        measured_fire_after.deferred_owners - measured_fire_before.deferred_owners,
        measured_fire_after.missing_chunks - measured_fire_before.missing_chunks,
        ignition.accepted,
        ignition.attempted,
    );
    println!(
        "  durability: rotations {rotations}, rotation-requested measured ticks {rotation_stall_ticks}, max checkpoint backlog {max_checkpoint_backlog}, max pending commits {max_pending_commits}, final pending actions {remaining_actions}, max backlog {max_backlog} ticks, longest backlog streak {max_backlog_streak} ticks, queue first/last {queue_window} means {queue_first:.2}/{queue_last:.2}; replant actions admitted/deferred {}/{}, replanted cells {}; wood initial/final {initial_wood}/{final_wood}, observed loss after replants {wood_loss}, burn+source-edit upper bound {wood_budget}; restart world/fire hashes {:016x}/{:016x} vs {:016x}/{:016x}; pass: {passed}",
        replanter.admitted,
        replanter.deferred,
        replanter.replanted,
        world_hash,
        fire_hash,
        restart_world_hash,
        restart_fire_hash,
    );
    println!(
        "  limits: forest/frontier prefill and replanting are benchmark-directed but WAL-durable; recurring glowstone uses the production fire seed and action WAL path, not a socket-authorized player command; tick CPU includes benchmark edit planning; no render/client frame timing"
    );
    if passed {
        Ok(())
    } else {
        Err(io::Error::other(
            "fire WAL soak acceptance criteria not met",
        ))
    }
}

fn forest_owners() -> Vec<ChunkKey> {
    (0..CHUNKS_Z)
        .flat_map(|z| (0..CHUNKS_X).map(move |x| ChunkKey { x, y: 4, z }))
        .collect()
}

fn ensure_resident(world: &mut World, owner: ChunkKey) -> io::Result<()> {
    if world.cached_version(owner).is_some() {
        return Ok(());
    }
    let (epoch, pending) = world.begin_chunk_load(owner)?;
    let loaded = if let Some(snapshot) = pending {
        world.load_chunk_snapshot_uncached(owner, &snapshot)?
    } else {
        world.load_chunk_uncached(owner)?
    };
    if !world.install_loaded_if_absent(loaded, epoch)? {
        return Err(io::Error::other("fire fixture chunk load was superseded"));
    }
    Ok(())
}

fn install_forest(state: &mut State, owners: &[ChunkKey]) -> io::Result<()> {
    for &owner in owners {
        ensure_resident(&mut state.world, owner)?;
    }
    for chunk in owners.chunks(FOREST_BATCH_CHUNKS) {
        let mut edits = Vec::with_capacity(chunk.len() * 4_096);
        for &owner in chunk {
            for local_y in 0..16 {
                for local_z in 0..16 {
                    for local_x in 0..16 {
                        edits.push((
                            owner.x * 16 + local_x,
                            owner.y * 16 + local_y,
                            owner.z * 16 + local_z,
                            WOOD,
                        ));
                    }
                }
            }
        }
        let prepared = state.world.prepare_edits(&edits)?;
        let action = fixture_action(prepared, None);
        if !state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .map_err(stage_error)?
        {
            return Err(io::Error::other("forest WAL edit was empty"));
        }
        drain_durable(state, TickId::new(1), false)?;
    }
    Ok(())
}

fn install_initial_frontiers(
    state: &mut State,
    owners: &[ChunkKey],
    tick: TickId,
) -> io::Result<()> {
    let mut remaining = BTreeMap::new();
    for &owner in owners {
        let mut cells = Vec::with_capacity(96);
        for y in 1..=6 {
            for z in [0, 1, 14, 15] {
                for x in [0, 1, 14, 15] {
                    cells.push((x + 16 * (z + 16 * y)) as u16);
                }
            }
        }
        remaining.insert(owner, cells);
    }
    while !remaining.is_empty() {
        let wave = state
            .fire
            .prepare_benchmark_frontier_wave(&remaining, tick)?;
        let admitted_owners: Vec<_> = wave.transactions.iter().map(|tx| tx.owner()).collect();
        if admitted_owners.is_empty() {
            return Err(io::Error::other("fire fixture could not select a lane"));
        }
        for transaction in &wave.transactions {
            for change in transaction.changes() {
                if change.before == change.after {
                    return Err(io::Error::other(format!(
                        "fire fixture owner {:?} has unchanged WAL key {:?}",
                        transaction.owner(),
                        change.key
                    )));
                }
            }
        }
        let before = state.fire.load_metrics().admitted_transactions;
        durable::fire::stage_wave(state, tick, wave)
            .map_err(|error| io::Error::other(format!("bootstrap fire frontier: {error}")))?;
        if state.fire.load_metrics().admitted_transactions != before + admitted_owners.len() as u64
        {
            return Err(io::Error::other("fire fixture WAL wave was deferred"));
        }
        drain_durable(state, tick, false)?;
        for owner in admitted_owners {
            remaining.remove(&owner);
        }
    }
    Ok(())
}

fn fixture_action(
    world_edits: Vec<crate::world::PreparedEdit>,
    fire_seed: Option<crate::server::fire::FireSeed>,
) -> CommitAction {
    CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits,
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed,
        clock_change: None,
        entities: None,
        entity_wakes: Vec::new(),
    }
}

#[derive(Default)]
struct IgnitionSource {
    next: usize,
    attempted: u64,
    accepted: u64,
    replaced_wood: u64,
}

impl IgnitionSource {
    fn attempt(&mut self, state: &mut State, tick: TickId) -> io::Result<()> {
        self.attempted += 1;
        let owner_index = self.next % ACTIVE_CHUNKS;
        let owner = ChunkKey {
            x: (owner_index as i32) % CHUNKS_X,
            y: 4,
            z: (owner_index as i32) / CHUNKS_X,
        };
        let slot = self.next / ACTIVE_CHUNKS;
        let y = 64 + (slot % 16) as i32;
        let x = owner.x * 16 + 2 + ((slot / 16) % 12) as i32;
        let z = owner.z * 16 + 2 + ((slot / 192) % 12) as i32;
        let before = state
            .world
            .cached_block(x, y, z)
            .ok_or_else(|| io::Error::other("fire source chunk is not resident"))?;
        if before == GLOWSTONE {
            self.next += 1;
            return Ok(());
        }
        let (source, local) = world_to_chunk(x, y, z);
        let cell = (local[0] + 16 * (local[2] + 16 * local[1])) as u16;
        let seed = match state
            .fire
            .prepare_seed_from_edit(tick, source, cell, GLOWSTONE)
        {
            Ok(Some(seed)) => seed,
            Ok(None) => return Err(io::Error::other("glowstone produced no fire seed")),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        };
        let prepared = state.world.prepare_edits(&[(x, y, z, GLOWSTONE)])?;
        let action = fixture_action(prepared, Some(seed.clone()));
        match state.durability.try_stage(tick, &action, None) {
            Ok(true) => {
                state.fire.mark_seed_submitted(&seed)?;
                self.accepted += 1;
                self.replaced_wood += u64::from(before == WOOD);
                self.next += 1;
            }
            Ok(false) => return Err(io::Error::other("glowstone edit WAL action was empty")),
            Err(StageError::Conflict | StageError::Full) => {}
            Err(error) => return Err(stage_error(error)),
        }
        Ok(())
    }
}

fn paced_tick(
    state: &mut State,
    tick: TickId,
    deadline: &mut Instant,
    work: impl FnOnce(&mut State, TickId) -> io::Result<()>,
) -> io::Result<(TickSample, Duration, Duration)> {
    let wait = deadline.saturating_duration_since(Instant::now());
    if !wait.is_zero() {
        thread::sleep(wait);
    }
    let now = Instant::now();
    let started = now;
    state.tick_backlog = now
        .saturating_duration_since(*deadline)
        .as_nanos()
        .checked_div(FIXED_STEP.as_nanos())
        .unwrap_or_default()
        .min(u64::MAX as u128) as u64;
    let preparation_started = Instant::now();
    work(state, tick)?;
    let preparation = preparation_started.elapsed();
    runtime::tick_with_inputs(state, tick, Instant::now(), Vec::new(), Vec::new())
        .map_err(|error| io::Error::other(format!("fire soak tick {}: {error}", tick.get())))?;
    *deadline += FIXED_STEP;
    let mut sample = state
        .metrics
        .latest()
        .ok_or_else(|| io::Error::other("fire tick emitted no metrics"))?;
    let runtime = sample.tick_total;
    sample.tick_total = started.elapsed();
    Ok((sample, preparation, runtime))
}

fn drain_durable(state: &mut State, tick: TickId, wait_rotation: bool) -> io::Result<()> {
    for _ in 0..MAX_DRAIN_POLLS {
        durable::process_durable_actions(state, tick, Instant::now())?;
        if state.durability.pending.is_empty()
            && state.durability.queued.is_empty()
            && (!wait_rotation
                || (!state.durability.rotation_requested
                    && state.durability.rotation_receipt.is_none()))
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(2));
    }
    Err(io::Error::other("fire WAL/checkpoint drain timed out"))
}

fn forest_wood_count(world: &mut World, owners: &[ChunkKey]) -> io::Result<u64> {
    let mut count = 0;
    for &owner in owners {
        let chunk = world
            .cached_arc_chunk(owner)
            .ok_or_else(|| io::Error::other("fire forest owner was evicted"))?;
        for index in 0..4_096 {
            count += u64::from(chunk.block_index(index) == Some(WOOD));
        }
    }
    Ok(count)
}

fn forest_hash(world: &mut World, owners: &[ChunkKey]) -> io::Result<u64> {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &owner in owners {
        let chunk = world
            .cached_arc_chunk(owner)
            .ok_or_else(|| io::Error::other("fire forest owner was evicted"))?;
        for coordinate in [owner.x, owner.y, owner.z] {
            hash_bytes(&mut hash, &coordinate.to_le_bytes());
        }
        hash_bytes(&mut hash, &chunk.version.to_le_bytes());
        for index in 0..4_096 {
            let block = chunk
                .block_index(index)
                .ok_or_else(|| io::Error::other("fire forest chunk is incomplete"))?;
            hash_bytes(&mut hash, &block.0.to_le_bytes());
        }
    }
    Ok(hash)
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x100_0000_01b3);
    }
}

fn percentiles(mut values: Vec<u64>) -> (u64, u64, u64, u64) {
    values.sort_unstable();
    let index = |percent: usize| (values.len() - 1) * percent / 100;
    (
        values[index(0)],
        values[index(50)],
        values[index(95)],
        values[index(99)],
    )
}

fn longest_backlog_streak(samples: &[TickSample]) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for sample in samples {
        if sample.backlog_ticks > 0 {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

fn mean_pending(samples: &[TickSample]) -> f64 {
    samples
        .iter()
        .map(|sample| sample.pending_durable_actions as f64)
        .sum::<f64>()
        / samples.len() as f64
}

fn stage_error(error: StageError) -> io::Error {
    io::Error::other(format!("fire fixture WAL stage: {error:?}"))
}
