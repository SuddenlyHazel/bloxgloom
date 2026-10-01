//! Fixed-step coordinator and phase barriers. The coordinator owns admission
//! and publication; validated disjoint owner commits may apply on workers
//! behind a barrier. Handlers only prepare results from immutable snapshots.
//!
//! Owner state is durable in the single main journal under the
//! `bloxgloom:owner_state` domain (`runtime::owner_codec`). There is exactly
//! one owner store — the barrier-owned `DurableOwnerStore` inside
//! `SystemRuntime` — and one WAL record per owner wave. A separate owner log
//! was deliberately rejected: two tails would make a commit spanning entity
//! state and owner state non-atomic and force recovery to reconcile two
//! tails. Rotation materializes the full latest-value map (every domain)
//! into the new base generation, so owner cells survive rotation with no
//! per-key checkpoint file; adding dirty-checkpoint entries without a
//! backing file would wedge the rotation gate instead of bounding the tail.
//!
//! Logical ticks consume confirmed state only. Motion's deterministic batch
//! barrier and each phase's last registered-owner admission must apply before
//! their boundary completes. These waits can stretch a tick in wall time;
//! the socket reactor remains independent. Ordinary asynchronous actions and
//! fire may still wait for a later receipt poll, never becoming speculative
//! inputs. No receipt timing selects a physics step or permits tick rollback.

use super::effects::{Effect, EffectBuffer, EffectLimits, route_effects};
use super::metrics::{Metric, TickSample};
use super::simulation::{CommandQueue, FixedStepClock, OrderKey, QueueError};
use super::*;

pub(in crate::server) mod adapters;
pub(in crate::server) mod owner_codec;
pub(in crate::server) mod owner_commit;
pub(in crate::server) mod owner_durable;
pub(in crate::server) mod owner_effects;
pub(in crate::server) mod owner_wake;
pub(in crate::server) mod systems;

#[cfg(test)]
#[path = "runtime/tests.rs"]
mod tests;

/// Trusted fixed-step inputs shared by registered server-owned admission
/// callbacks. Mod handlers never receive this context; their API is the
/// immutable `OwnerJob`/scratch-patch path in `registry`.
pub(in crate::server) struct CoordinatorContext<'a> {
    state: &'a mut State,
    tick: TickId,
    now: Instant,
    rejected: Option<Vec<SimulationInput>>,
    ready: Option<Vec<SimulationInput>>,
    movement_load: movement::WorkerLoad,
}

#[tracing::instrument(name = "server_simulation", skip_all)]
pub(super) fn run_simulation_ticks(
    mut state: State,
    input: Receiver<SimulationInput>,
) -> io::Result<()> {
    let mut clock = FixedStepClock::after(TickId::new(state.recovered_tick));
    let mut commands = CommandQueue::new(INPUT_CAPACITY);
    let mut last_clock = Instant::now();
    let mut next_control_sequence = 0u64;
    let mut report_at = Instant::now();
    loop {
        thread::sleep(FIXED_STEP.saturating_sub(clock.backlog()));
        let now = Instant::now();
        if let Some(elapsed_ms) = state.world_time.poll() {
            for client in state.clients.values() {
                // The next sample repairs a dropped advisory clock update.
                let _ = client
                    .sender
                    .try_send(ServerMessage::WorldTime { elapsed_ms });
            }
        }
        let elapsed = now.duration_since(last_clock);
        last_clock = now;
        let batch = clock
            .advance(elapsed, MAX_CATCH_UP_TICKS)
            .map_err(|error| io::Error::other(format!("simulation clock exhausted: {error:?}")))?;
        for tick in batch.ticks {
            state.tick_backlog = batch.backlog_ticks;
            let mut disconnected = false;
            let mut rejected = Vec::new();
            // Queue capacity absorbs bursts; tick work remains separately
            // bounded so a burst of socket input cannot monopolize a frame.
            for _ in 0..MAX_INPUTS_PER_TICK {
                let command = match input.try_recv() {
                    Ok(command) => command,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                };
                let key = match &command {
                    SimulationInput::Join { .. } => {
                        next_control_sequence = next_control_sequence
                            .checked_add(1)
                            .ok_or_else(|| io::Error::other("join ordering sequence exhausted"))?;
                        OrderKey::new(tick, 0, next_control_sequence)
                    }
                    SimulationInput::Command { id, sequence, .. }
                    | SimulationInput::Leave { id, sequence } => {
                        OrderKey::new(tick, *id, *sequence)
                    }
                };
                if let Err(error) = commands.try_push(key, command) {
                    let payload = match error {
                        QueueError::Full { payload, .. }
                        | QueueError::DuplicateKey { payload, .. }
                        | QueueError::ClosedTick { payload, .. } => payload,
                    };
                    rejected.push(payload);
                }
            }
            let ready = commands
                .drain_tick(tick)
                .expect("simulation command ticks are drained in order");
            if let Err(error) = tick_with_inputs(
                &mut state,
                tick,
                now,
                rejected,
                ready.into_iter().map(|command| command.payload).collect(),
            ) {
                for client in state.clients.values() {
                    let _ = client.socket.shutdown(Shutdown::Both);
                }
                return Err(io::Error::new(
                    error.kind(),
                    format!("simulation tick {}: {error}", tick.get()),
                ));
            }
            if state.durability.failed {
                tracing::error!(
                    tick = tick.get(),
                    "authoritative durability failed; stopping simulation coordinator"
                );
                for client in state.clients.values() {
                    let _ = client.socket.shutdown(Shutdown::Both);
                }
                return Err(io::Error::other("authoritative durability failed"));
            }
            if disconnected {
                state.save_connected_positions()?;
                state.world_time.finish()?;
                return Ok(());
            }
        }
        if report_at.elapsed() >= Duration::from_secs(5) {
            if let Some(summary) = state.metrics.summary(Metric::TickTotalNanos)
                && (batch.backlog_ticks > 0 || summary.p95 > 15_000_000)
            {
                let phase_p95: [f64; metrics::PHASE_COUNT] = std::array::from_fn(|index| {
                    state
                        .metrics
                        .summary(Metric::PhaseNanos(index))
                        .map_or(0.0, |phase| phase.p95 as f64 / 1_000_000.0)
                });
                let latest = state.metrics.latest().unwrap_or_default();
                let latency_p95 = |event| {
                    state
                        .metrics
                        .latency_summary(event)
                        .map_or(0.0, |latency| latency.p95 as f64 / 1_000_000.0)
                };
                let movement_utilization = state
                    .metrics
                    .movement_worker_utilization_percent()
                    .map_or_else(|| "n/a".to_owned(), |percent| format!("{percent:.1}%"));
                tracing::warn!(
                    tick_p95_ms = summary.p95 as f64 / 1_000_000.0,
                    tick_p99_ms = summary.p99 as f64 / 1_000_000.0,
                    lagged_ticks = state.metrics.lagged_samples(),
                    sampled_ticks = summary.samples,
                    backlog_ticks = batch.backlog_ticks,
                    backlog_ms = batch.backlog.as_secs_f64() * 1000.0,
                    ?phase_p95,
                    %movement_utilization,
                    input_queue = latest.input_queue_depth,
                    pending_actions = latest.pending_durable_actions,
                    pending_snapshots = latest.pending_world_snapshots,
                    wal_bytes = latest.wal_tail_bytes,
                    loader_jobs = latest.loader_outstanding,
                    resident_chunks = latest.resident_chunks,
                    pinned_chunks = latest.pinned_chunks,
                    clients = latest.active_clients,
                    drops = latest.active_drops,
                    outbound_frames = latest.replication_queue_depth,
                    outbound_capacity = latest.replication_queue_capacity,
                    outbound_bytes_queued = latest.replication_bytes_queued,
                    outbound_bytes_sent = latest.replication_bytes_sent,
                    outbound_rejections = latest.replication_queue_rejections,
                    wal_latency_p95_ms = latency_p95(metrics::LatencyEvent::DurableWalReceipt),
                    load_latency_p95_ms = latency_p95(metrics::LatencyEvent::ChunkLoad),
                    barrier_latency_p95_ms = latency_p95(metrics::LatencyEvent::PhaseBarrierWait),
                    "simulation running behind"
                );
            }
            report_at = Instant::now();
        }
    }
}

fn reject_simulation_input(state: &mut State, input: SimulationInput) {
    match input {
        SimulationInput::Join { reply, .. } => {
            let _ = reply.try_send(JoinResponse::Completed(Box::new(Err(io::Error::new(
                ErrorKind::WouldBlock,
                "server input queue full",
            )))));
        }
        SimulationInput::Command { id, .. } => {
            if let Some(client) = state.clients.get(&id) {
                let _ = client.socket.shutdown(Shutdown::Both);
            }
        }
        SimulationInput::Leave { id, .. } => {
            state.remove_client(id);
        }
    }
}

fn apply_simulation_input(state: &mut State, input: SimulationInput, tick: TickId) {
    match input {
        SimulationInput::Join {
            guard,
            name,
            profile,
            inventory,
            sender,
            socket,
            reply,
        } => {
            if state.pending_joins.len() >= state.admission_limit {
                let _ = reply.try_send(JoinResponse::Completed(Box::new(Err(io::Error::new(
                    ErrorKind::WouldBlock,
                    "server join queue full",
                )))));
            } else {
                state.pending_joins.push_back(PendingJoin {
                    guard,
                    name,
                    profile,
                    inventory: *inventory,
                    sender,
                    socket,
                    reply,
                    queued_at: Instant::now(),
                });
            }
        }
        SimulationInput::Command { id, message, .. } => {
            if let Err(error) = handle_live_message(state, id, message, tick) {
                tracing::warn!(%error, player_id = id, tick = tick.get(), "client command failed");
                if let Some(client) = state.clients.get(&id) {
                    let _ = client.socket.shutdown(Shutdown::Both);
                }
            }
        }
        SimulationInput::Leave { id, .. } => {
            state.remove_client(id);
        }
    }
}

fn process_pending_joins(state: &mut State, tick: TickId) {
    const JOIN_DEFER_TIMEOUT: Duration = Duration::from_secs(4);
    let attempts = state.pending_joins.len();
    for _ in 0..attempts {
        let Some(join) = state.pending_joins.pop_front() else {
            break;
        };
        if join.guard.cancelled() {
            continue;
        }
        if join.queued_at.elapsed() >= JOIN_DEFER_TIMEOUT {
            join.guard.cancel();
            let _ = join
                .reply
                .try_send(JoinResponse::Completed(Box::new(Err(io::Error::new(
                    ErrorKind::TimedOut,
                    "authoritative spawn terrain unavailable",
                )))));
            continue;
        }
        if !state
            .durability
            .inventory_overlay
            .contains_key(&join.profile)
            && state
                .durability
                .inventory_revisions
                .get(&join.profile)
                .is_some_and(|revision| *revision != join.inventory.revision)
        {
            let _ = join.reply.try_send(JoinResponse::RefreshInventory);
            continue;
        }
        // Granting an epoch is itself durable: it closes the prior action
        // namespace. Never grant a second epoch while that profile is live.
        if state
            .clients
            .values()
            .any(|client| client.profile == join.profile)
        {
            let _ = join
                .reply
                .try_send(JoinResponse::Completed(Box::new(Err(io::Error::new(
                    ErrorKind::AlreadyExists,
                    "profile already connected",
                )))));
            continue;
        }
        let action_epoch =
            match state.durability.request_epoch_grant(join.profile, tick) {
                Ok(Some(epoch)) => epoch,
                Ok(None) | Err(durable::StageError::Conflict | durable::StageError::Full) => {
                    state.pending_joins.push_back(join);
                    continue;
                }
                Err(error) => {
                    let _ = join.reply.try_send(JoinResponse::Completed(Box::new(Err(
                        io::Error::other(format!("action session grant failed: {error:?}")),
                    ))));
                    continue;
                }
            };
        match join_named_client(
            state,
            join.profile,
            &join.name,
            action_epoch,
            join.inventory.clone(),
            join.sender.clone(),
            &join.socket,
        ) {
            Ok(reply) => {
                let id = reply.id;
                if !join.guard.admit()
                    || join
                        .reply
                        .try_send(JoinResponse::Completed(Box::new(Ok(reply))))
                        .is_err()
                {
                    state.remove_client(id);
                } else {
                    let claimed = state.durability.claim_epoch_grant(join.profile);
                    debug_assert_eq!(claimed, Some(action_epoch));
                    players::joined(state, id);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                state.pending_joins.push_back(join);
            }
            Err(error) => {
                let _ = join
                    .reply
                    .try_send(JoinResponse::Completed(Box::new(Err(error))));
            }
        }
    }
}

#[cfg(test)]
pub(super) fn tick_once(state: &mut State, tick: TickId, now: Instant) -> io::Result<()> {
    tick_with_inputs(state, tick, now, Vec::new(), Vec::new())
}

pub(super) fn tick_with_inputs(
    state: &mut State,
    tick: TickId,
    now: Instant,
    rejected: Vec<SimulationInput>,
    ready: Vec<SimulationInput>,
) -> io::Result<()> {
    let tick_started = Instant::now();
    let input_queue_depth = rejected.len() + ready.len();
    let mut phase_times = [Duration::ZERO; metrics::PHASE_COUNT];
    streaming::poll_chunk_loads(state)?;
    let mut context = CoordinatorContext {
        state,
        tick,
        now,
        rejected: Some(rejected),
        ready: Some(ready),
        movement_load: movement::WorkerLoad::default(),
    };
    for (phase_index, phase) in Phase::ALL.into_iter().enumerate() {
        let phase_started = Instant::now();
        if phase == Phase::Publish {
            players::publish_roster(context.state);
            players::delivery::publish(context.state)?;
        }
        if phase == Phase::Simulation {
            players::drive(context.state, tick)?;
        }
        let system_count = context.state.phase_plan.systems(phase).len();
        if system_count == 0 {
            return Err(io::Error::other(format!(
                "no systems registered for {phase:?}"
            )));
        }
        // Registered owner waves stage without blocking so every wave's
        // fsync is in flight before any receipt is polled: disjoint waves
        // never block each other's submission, and overlapping ones defer
        // deterministically at stage time. Trusted drivers run inline in
        // plan order between stages; the whole staged set drains at the
        // phase barrier through the single shared apply gate.
        let mut barrier = None;
        // Staged key sets in canonical stage order,
        // for arbitration of each newly prepared wave.
        let mut staged_key_sets: Vec<Vec<super::journal::StateKey>> = Vec::new();
        for index in 0..system_count {
            let registered = context.state.phase_plan.systems(phase)[index].clone();
            if let Some(driver) = registered.driver() {
                if let Err(error) = driver(&mut context) {
                    if let Some(barrier) = barrier {
                        durable::complete_barrier(context.state, barrier)?;
                    }
                    return Err(error);
                }
            } else {
                let batch_wave = u16::try_from(index)
                    .map_err(|_| io::Error::other("too many registered phase systems"))?;
                let effect_kinds = Arc::clone(&context.state.effect_kinds);
                let players: Vec<_> = context
                    .state
                    .clients
                    .values()
                    .map(|client| client.position())
                    .collect();
                let seed = context.state.seed;
                let entities = &context.state.entities;
                // Split the borrows: the owner wave stages through the
                // shared durable journal while applying to the runtime store.
                let (system_runtime, durability, world) = (
                    &mut context.state.system_runtime,
                    &mut context.state.durability,
                    &mut context.state.world,
                );
                let mut missing = Vec::new();
                let result = system_runtime.stage_registered_wave_with_world(
                    &registered,
                    tick,
                    batch_wave,
                    systems::RegisteredWaveInputs {
                        effects: &effect_kinds,
                        durability,
                        in_flight: &staged_key_sets,
                        world: systems::RegisteredWorldInputs {
                            world: Some(world),
                            entities: Some(entities),
                            lifecycles: Some(&context.state.lifecycles),
                            players: &players,
                            seed,
                            missing: &mut missing,
                        },
                    },
                );
                for key in missing {
                    let _ = streaming::request_chunk(context.state, key)?;
                }
                match result {
                    Ok(Some(wave)) => {
                        staged_key_sets.push(wave.keys().to_vec());
                        barrier = Some(wave.barrier());
                    }
                    Ok(None) => {}
                    Err(error)
                        if registered.world_read_radius().is_some()
                            && error.kind() == io::ErrorKind::WouldBlock =>
                    {
                        // The wave retained its owner/deadline and did not
                        // publish a cursor. Retry behind the earlier receipt
                        // instead of tearing down the coordinator.
                    }
                    Err(error) => {
                        // The failed wave staged nothing, but earlier waves
                        // are already submitted: drain them so their receipted
                        // records still apply before the deferral propagates.
                        if let Some(barrier) = barrier {
                            durable::complete_barrier(context.state, barrier)?;
                        }
                        return Err(error);
                    }
                }
            }
        }
        if let Some(barrier) = barrier {
            durable::complete_barrier(context.state, barrier)?;
        }
        phase_times[phase_index] = phase_started.elapsed();
    }
    let movement_load = context.movement_load;
    let state = context.state;
    let outbound = state.outbound.snapshot();
    let bytes_sent = outbound.sent_bytes.saturating_sub(state.last_sent_bytes);
    let queue_rejections = outbound.rejections.saturating_sub(state.last_rejections);
    state.last_sent_bytes = outbound.sent_bytes;
    state.last_rejections = outbound.rejections;
    let entity_mirror = state.durability.entity_mirror.metrics();
    let sample = TickSample {
        tick_id: tick.get(),
        tick_total: tick_started.elapsed(),
        phases: phase_times,
        backlog_ticks: state.tick_backlog,
        input_queue_depth: input_queue_depth as u64,
        pending_durable_actions: (state.durability.pending.len() + state.durability.queued.len())
            as u64,
        pending_world_snapshots: state.world.pending_snapshot_count() as u64,
        wal_tail_bytes: state.durability.writer.bytes(),
        wal_rotations: state.durability.completed_rotations,
        loader_outstanding: state.loader.outstanding() as u64,
        resident_chunks: state.world.resident_chunk_count() as u64,
        pinned_chunks: state.world.pinned_chunk_count() as u64,
        active_clients: state.clients.len() as u64,
        active_drops: crate::server::drops::airborne_count(&state.entities) as u64,
        entity_mirror_outstanding: entity_mirror.outstanding as u64,
        entity_mirror_high_water: entity_mirror.high_water as u64,
        entity_mirror_applied_sequence: entity_mirror.applied_sequence,
        entity_mirror_checkpoint_sequence: entity_mirror.checkpoint_sequence,
        movement_worker_busy_nanos: movement_load.busy.as_nanos().min(u64::MAX as u128) as u64,
        movement_worker_capacity_nanos: movement_load.capacity.as_nanos().min(u64::MAX as u128)
            as u64,
        replication_bytes_queued: outbound.queued_bytes,
        replication_bytes_sent: bytes_sent,
        replication_queue_depth: outbound.queued_messages,
        replication_queue_capacity: (state.clients.len() * OUTBOUND_CAPACITY) as u64,
        replication_queue_rejections: queue_rejections,
    };
    state.metrics.record(sample);
    #[cfg(test)]
    {
        let motion = std::mem::take(&mut state.motion_metrics);
        if let Some(observer) = &state.motion_observer {
            let _ = observer.try_send(motion);
        }
    }
    if let Some(observer) = &state.tick_observer {
        observer
            .try_send(sample)
            .map_err(|error| io::Error::other(format!("tick observer did not drain: {error}")))?;
    }
    Ok(())
}

fn commit_block_effects(state: &mut State, tick: TickId) -> io::Result<()> {
    if state.pending_block_changes.is_empty() {
        return Ok(());
    }
    let mut output = EffectBuffer::new(tick, 0, state.pending_block_changes.len())
        .map_err(|error| io::Error::other(format!("block effect buffer: {error:?}")))?;
    for &cell in &state.pending_block_changes {
        output
            .emit(Effect::BlockChanged { cell })
            .map_err(|error| io::Error::other(format!("block effect emission: {error:?}")))?;
    }
    let batch = route_effects(
        output
            .finish()
            .map_err(|error| io::Error::other(format!("block effect buffer: {error:?}")))?,
        EffectLimits::default(),
    )
    .map_err(|error| io::Error::other(format!("block effect routing: {error:?}")))?;
    debug_assert_eq!(batch.commit_phase(), Phase::InteractionCommit);
    // Suspended policies and terrain-reactive scheduled policies get latency
    // hints. Their recheck index or persisted deadline remains the fallback
    // across the edit/notification crash window. Bound traversal as well as
    // retained hints; never collect the entire affected entity population.
    for owner in batch.owners().iter().take(16) {
        for id in state.entities.terrain_wake_ids(owner.owner, 16) {
            state.durability.hint_entity_wake(id);
        }
    }
    state.pending_block_changes.clear();
    Ok(())
}
