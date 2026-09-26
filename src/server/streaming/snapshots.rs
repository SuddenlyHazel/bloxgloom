//! One bounded publication barrier for immutable confirmed chunk snapshots.
//!
//! No state is applied and no input is consumed between capture and this
//! barrier. Socket I/O continues independently. Consequently later commits
//! cannot overtake a snapshot, and no retained delta log or cross-tick retry
//! queue is needed. Unselected/backpressured chunks remain unsent and eligible.

use super::{can_stream_snapshot_size, entities};
use crate::server::entities::EntityError;
use crate::server::outbound::SharedMessage;
use crate::server::parallel::{BatchId, JobKey, JobOutcome, PhaseExecutor, SubmitError};
use crate::server::simulation::{Phase, TickId};
use crate::server::{Client, State};
use crate::world::ChunkKey;
use std::collections::BTreeMap;
use std::io;
use std::net::Shutdown;
use std::sync::Arc;

// Bounds capture, jobs, queued results and transient shared pages, not just
// emitted frames. Client rotation in publish_streams gives distinct chunks a
// turn even when all preceding clients continue expanding their interest.
const MAX_SNAPSHOT_JOBS: usize = 16;

#[derive(Clone, Copy)]
struct Target {
    session: u64,
    center: ChunkKey,
    radius: u8,
}

#[derive(Default)]
pub(super) struct Selection {
    groups: BTreeMap<(ChunkKey, u64), Vec<Target>>,
}

impl Selection {
    pub(super) fn select(&mut self, session: u64, client: &Client, key: ChunkKey) {
        let identity = (key, client.next_snapshot_epoch);
        if self.groups.len() == MAX_SNAPSHOT_JOBS && !self.groups.contains_key(&identity) {
            return;
        }
        self.groups.entry(identity).or_default().push(Target {
            session,
            center: client.center,
            radius: client.radius,
        });
    }
}

pub(in crate::server) struct Workers {
    executor: PhaseExecutor<Prepared, entities::SnapshotError>,
    generation: u64,
    targets: BTreeMap<JobKey, Vec<Target>>,
    oversized: Vec<Target>,
}

impl Workers {
    pub(in crate::server) fn new(worker_count: usize) -> io::Result<Self> {
        Ok(Self {
            executor: PhaseExecutor::new(worker_count, MAX_SNAPSHOT_JOBS, MAX_SNAPSHOT_JOBS)
                .map_err(|error| io::Error::other(format!("snapshot worker pool: {error:?}")))?,
            generation: 0,
            targets: BTreeMap::new(),
            oversized: Vec::new(),
        })
    }
}

struct Prepared {
    key: ChunkKey,
    epoch: u64,
    block_revision: u64,
    entity_revision: u64,
    targets: Vec<Target>,
    frames: Vec<Arc<SharedMessage>>,
    bytes: u64,
    #[cfg(test)]
    worker: std::thread::ThreadId,
}

pub(super) fn publish(state: &mut State, selection: Selection) -> io::Result<()> {
    // Dropping a slow client during stream selection can queue a player
    // despawn after the tick's committed-effects pass. A new snapshot must not
    // jump ahead of those older queued revisions. Leave it unsent until the
    // next ordered effects pass; no capture or retained job is necessary.
    if !state.durability.publish_queue.is_empty() {
        return Ok(());
    }
    let batch = dispatch(state, selection)?;
    finish(state, batch)
}

fn dispatch(state: &mut State, selection: Selection) -> io::Result<BatchId> {
    debug_assert!(state.durability.publish_queue.is_empty());
    let workers = &mut state.snapshot_workers;
    workers.generation = workers
        .generation
        .checked_add(1)
        .ok_or_else(|| io::Error::other("snapshot generation exhausted"))?;
    // Executor-local generation, not a simulation tick: tests and resync may
    // publish more than once at a logical tick. It never survives restart.
    let batch = BatchId::new(TickId::new(workers.generation), Phase::Publish, 0);
    for ((key, epoch), targets) in selection.groups {
        let Some(chunk) = state.world.cached_arc_chunk(key) else {
            continue;
        };
        let capture = (|| {
            let mut views = state.entities.public_views_for_chunk_bounded_bytes(
                key,
                entities::MAX_PUBLIC_ENTITIES_PER_CHUNK,
                entities::MAX_PUBLIC_ENTITY_BYTES_PER_CHUNK,
            )?;
            let remaining = entities::MAX_PUBLIC_ENTITIES_PER_CHUNK - views.len();
            // Session payloads have a fixed four-byte bound and at most 256 entries.
            views.extend(
                state
                    .player_entities
                    .public_views_for_chunk_bounded(key, remaining)?,
            );
            Ok::<_, EntityError>(views)
        })();
        let views = match capture {
            Ok(views) => views,
            Err(EntityError::SpatialQueryTooBroad) => {
                workers.oversized.extend(targets);
                continue;
            }
            Err(error) => {
                // Even corruption must not leave previously accepted jobs in
                // an open batch if the caller handles the error.
                let _ = workers.executor.barrier(batch);
                workers.targets.clear();
                workers.oversized.clear();
                return Err(io::Error::other(error));
            }
        };
        let catalog = state.world.catalog_arc();
        let block_revision = chunk.version;
        let entity_revision = state.entity_public_revision;
        let job = JobKey::new(batch, key, epoch, block_revision);
        let failed_targets = targets.clone();
        let submitted = workers.executor.try_submit(job, move |_| {
            let messages = entities::snapshot_messages(
                (*chunk).clone(),
                epoch,
                entity_revision,
                views,
                &catalog,
            )?;
            let frames: Vec<_> = messages.into_iter().map(SharedMessage::new).collect();
            let bytes = frames.iter().map(|frame| frame.wire_len() as u64).sum();
            // Validate a whole snapshot before retaining it in the result queue.
            can_stream_snapshot_size(Default::default(), frames.len(), bytes)?;
            Ok(Prepared {
                key,
                epoch,
                block_revision,
                entity_revision,
                targets,
                frames,
                bytes,
                #[cfg(test)]
                worker: std::thread::current().id(),
            })
        });
        match submitted {
            Ok(()) => {
                workers.targets.insert(job, failed_targets);
            }
            Err(SubmitError::QueueSaturated { .. }) => {} // remains unsent
            Err(error) => {
                let _ = workers.executor.barrier(batch);
                workers.targets.clear();
                workers.oversized.clear();
                return Err(io::Error::other(format!("snapshot dispatch: {error:?}")));
            }
        }
    }
    Ok(batch)
}

fn finish(state: &mut State, batch: BatchId) -> io::Result<()> {
    let results = state
        .snapshot_workers
        .executor
        .barrier(batch)
        .map_err(|error| io::Error::other(format!("snapshot barrier: {error:?}")))?;
    let mut disconnect = std::mem::take(&mut state.snapshot_workers.oversized);
    let mut targets = std::mem::take(&mut state.snapshot_workers.targets);
    let mut fatal = None;
    for owner in results.owners {
        for job in owner.jobs {
            match job.outcome {
                JobOutcome::Completed(prepared) => {
                    if let Err(error) = apply(state, prepared) {
                        fatal = Some(error);
                    }
                }
                JobOutcome::Failed(entities::SnapshotError::Invalid(error)) => {
                    fatal = Some(error);
                }
                JobOutcome::Failed(entities::SnapshotError::Capacity) | JobOutcome::Panicked(_) => {
                    disconnect.extend(targets.remove(&job.key).unwrap_or_default());
                }
                JobOutcome::Cancelled | JobOutcome::Stale => {}
            }
        }
    }
    for target in disconnect {
        if let Some(client) = state.clients.get(&target.session) {
            let _ = client.socket.shutdown(Shutdown::Both);
        }
        // An impossible complete snapshot is not an endlessly retried request.
        // No partial epoch was queued; other sessions/groups remain usable.
        state.remove_client(target.session);
    }
    if let Some(error) = fatal {
        return Err(error);
    }
    Ok(())
}

fn apply(state: &mut State, prepared: Prepared) -> io::Result<()> {
    #[cfg(test)]
    assert_ne!(prepared.worker, std::thread::current().id());
    let Prepared {
        key,
        epoch,
        block_revision,
        entity_revision,
        targets,
        frames,
        bytes,
        ..
    } = prepared;
    // Redundant under the live barrier, intentional at the result boundary:
    // future asynchronous callers cannot publish an old capture as current.
    if state.world.cached_version(key) != Some(block_revision)
        || state.entity_public_revision != entity_revision
    {
        return Ok(());
    }
    let next_epoch = epoch
        .checked_add(1)
        .ok_or_else(|| io::Error::other("client snapshot epoch exhausted"))?;
    for target in targets {
        let Some(client) = state.clients.get_mut(&target.session) else {
            continue;
        };
        if client.center != target.center
            || client.radius != target.radius
            || client.next_snapshot_epoch != epoch
            || client.sent.contains(&key)
            || !client.interested(key)
        {
            continue;
        }
        if !can_stream_snapshot_size(client.sender.snapshot(), frames.len(), bytes)? {
            continue;
        }
        let mut healthy = true;
        for frame in &frames {
            if client.sender.try_send_shared(Arc::clone(frame)).is_err() {
                let _ = client.socket.shutdown(Shutdown::Both);
                healthy = false;
                break;
            }
        }
        if !healthy {
            state.remove_client(target.session);
            continue;
        }
        if !state.world.pin_resident_chunk(key) {
            return Err(io::Error::other(
                "snapshot chunk vanished before subscription pin",
            ));
        }
        client.next_snapshot_epoch = next_epoch;
        client.sent.insert(key);
        client.sent_epochs.insert(key, epoch);
        client.sent_block_versions.insert(key, block_revision);
        client.sent_entity_revisions.insert(key, entity_revision);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
