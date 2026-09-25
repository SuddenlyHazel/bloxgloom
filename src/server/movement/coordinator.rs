//! Coordinator-side movement dispatch and deterministic phase-barrier commit.

use super::*;
use crate::server::metrics::LatencyEvent;
use crate::server::parallel::{BatchId, JobKey, JobOutcome};
use crate::server::simulation::{Phase, TickId};
use crate::server::streaming::request_chunk;
use crate::server::{ServerMessage, State, world_to_chunk};
use crate::world::ChunkKey;
use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::time::Instant;

pub(crate) fn advance_players(state: &mut State, tick: TickId) -> io::Result<WorkerLoad> {
    let mut active = Vec::new();
    for (&id, client) in &mut state.clients {
        if client.pending_moves.is_empty() {
            client.movement.advance_idle_tick();
        } else {
            active.push(id);
        }
    }
    if active.is_empty() {
        return Ok(WorkerLoad::default());
    }
    active.sort_unstable();

    // A 250 ms burst moves at most 2.5 blocks. The 3x3x3 neighborhood
    // contains every possible collision sample around the current chunk;
    // absent chunks are left out so the pure resolver can identify the exact
    // one to request. Clustered players share the same immutable Arcs.
    let mut captured = HashSet::new();
    let mut chunks = Vec::new();
    for &id in &active {
        let position = state.clients[&id].position();
        let center = world_to_chunk(
            position[0].floor() as i32,
            position[1].floor() as i32,
            position[2].floor() as i32,
        )
        .0;
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let (Some(x), Some(y), Some(z)) = (
                        center.x.checked_add(dx),
                        center.y.checked_add(dy),
                        center.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    let key = ChunkKey { x, y, z };
                    if captured.insert(key)
                        && let Some(chunk) = state.world.cached_arc_chunk(key)
                    {
                        chunks.push(chunk);
                    }
                }
            }
        }
    }
    let view = Arc::new(
        VoxelView::from_resident_chunks_in(chunks, state.world.catalog_arc())
            .map_err(|error| io::Error::other(format!("resident voxel view: {error:?}")))?,
    );
    let batch_id = BatchId::new(tick, Phase::Simulation, 0);
    let dispatch_started = Instant::now();
    for &id in &active {
        let client = &state.clients[&id];
        let commands: Vec<_> = client
            .pending_moves
            .iter()
            .take(MAX_COMMANDS_PER_TICK + 1)
            .copied()
            .collect();
        let movement = client.movement;
        let view = Arc::clone(&view);
        let key = JobKey::new(
            batch_id,
            client.center,
            id,
            state.world.cached_version(client.center).unwrap_or(0),
        );
        state
            .movement_executor
            .try_submit(key, move |_| {
                Ok(process_movement_batch(&view, movement, &commands))
            })
            .map_err(|error| io::Error::other(format!("movement worker submit: {error:?}")))?;
    }
    let barrier_started = Instant::now();
    let results = state
        .movement_executor
        .barrier(batch_id)
        .map_err(|error| io::Error::other(format!("movement phase barrier: {error:?}")))?;
    let worker_load = WorkerLoad {
        busy: results.worker_run_time(),
        capacity: dispatch_started
            .elapsed()
            .saturating_mul(state.movement_executor.worker_count() as u32),
    };
    state
        .metrics
        .record_latency(LatencyEvent::PhaseBarrierWait, barrier_started.elapsed());
    if !view.revisions_match(|key| state.world.cached_version(key)) {
        for id in active {
            state
                .clients
                .get_mut(&id)
                .unwrap()
                .movement
                .advance_idle_tick();
        }
        return Ok(worker_load);
    }

    let mut missing = HashSet::new();
    let mut disconnected = Vec::new();
    let mut player_positions = Vec::with_capacity(active.len());
    for owner in results.owners {
        for job in owner.jobs {
            let id = job.key.job_id;
            let JobOutcome::Completed(batch) = job.outcome else {
                return Err(io::Error::other(format!(
                    "movement worker failed for client {id}"
                )));
            };
            let Some(client) = state.clients.get_mut(&id) else {
                return Err(io::Error::other(
                    "movement client disappeared before barrier",
                ));
            };
            for _ in 0..batch.consumed {
                client.pending_moves.pop_front();
            }
            client.movement = batch.state;
            let [x, y, z] = client.position();
            client.center = world_to_chunk(x.floor() as i32, y.floor() as i32, z.floor() as i32).0;
            player_positions.push((id, client.position()));
            // Position acknowledgements are cumulative: one authoritative
            // final position acknowledges every earlier command in this tick.
            // Sending each intermediate correction needlessly fills a
            // reliable outbound queue when a client is rendering chunks.
            if let Some(acknowledgment) = batch.acknowledgments.last() {
                let [x, y, z] = acknowledgment.position;
                if !client.enqueue(ServerMessage::Position {
                    ack_seq: acknowledgment.seq,
                    x,
                    y,
                    z,
                }) {
                    disconnected.push(id);
                }
            }
            if let Some(chunk) = batch.first_missing_chunk {
                missing.insert(chunk.key);
            }
        }
    }
    for id in disconnected {
        state.remove_client(id);
    }
    player_positions.sort_unstable_by_key(|(id, _)| *id);
    let mut player_deltas = Vec::with_capacity(player_positions.len());
    for (id, position) in player_positions {
        // In-process movement fixtures may construct a Client directly. Every
        // network join installs a corresponding player entity before the
        // session is exposed.
        if state.player_entities.id_for_session(id).is_some()
            && let Some(delta) = state
                .player_entities
                .update_position(id, position)
                .map_err(io::Error::other)?
        {
            player_deltas.push(delta);
        }
    }
    state.queue_player_entity_deltas(player_deltas)?;
    let mut missing: Vec<_> = missing.into_iter().collect();
    missing.sort_unstable_by_key(|key| (key.x, key.y, key.z));
    for key in missing {
        if !request_chunk(state, key)? {
            break;
        }
    }
    Ok(worker_load)
}
