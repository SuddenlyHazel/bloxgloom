//! Independent bounded distant interest, generation, revision fencing, and delivery.
mod disk;
mod scheduling;
mod sources;
mod worker;
use super::{ClientMessage, ServerMessage, State};
use crate::lod::{LodTile, TileKey};
use crate::world::{ChunkKey, World};
use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
const MAX_PENDING: usize = 4;
const MAX_CLIENT_REQUESTS: usize = 8;
const MAX_CACHE: usize = 64;
#[derive(Clone, Copy)]
struct Request {
    id: u64,
    key: TileKey,
    accepted: Instant,
}
#[derive(Default)]
struct Interest {
    horizon: u16,
    requests: VecDeque<Request>,
}
pub(super) struct Service {
    jobs: Option<SyncSender<worker::Job>>,
    results: Receiver<worker::Completion>,
    workers: Vec<JoinHandle<()>>,
    pending: HashMap<TileKey, (u64, Arc<AtomicBool>)>,
    cache: HashMap<TileKey, LodTile>,
    order: VecDeque<TileKey>,
    clients: HashMap<u64, Interest>,
    revision: u64,
    exhausted: bool,
    cursor: u64,
}
impl Service {
    pub(super) fn new(world: &World, root: std::path::PathBuf) -> io::Result<Self> {
        Self::with_workers(
            world,
            root,
            crate::lod::loading::worker_count("BLOXGLOOM_LOD_GENERATION_WORKERS"),
        )
    }
    pub(super) fn with_workers(
        world: &World,
        root: std::path::PathBuf,
        count: usize,
    ) -> io::Result<Self> {
        if !(1..=4).contains(&count) {
            return Err(io::Error::other("invalid LOD worker count"));
        }
        let (jobs, receiver) = mpsc::sync_channel(MAX_PENDING);
        let (sender, results) = mpsc::sync_channel(MAX_PENDING);
        let receiver = Arc::new(Mutex::new(receiver));
        let disk = Arc::new(Mutex::new(()));
        let mut workers = Vec::new();
        for lane in 0..count {
            let world = world.loader_view();
            let root = root.clone();
            let receiver = receiver.clone();
            let sender = sender.clone();
            let disk = disk.clone();
            match thread::Builder::new()
                .name(format!("lod-terrain-{lane}"))
                .spawn(move || worker::run(world, root, receiver, sender, disk))
            {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    drop(jobs);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        tracing::info!(target: "bloxgloom::lod_loading", workers=count, admitted=MAX_PENDING, "LOD generation pool ready");
        Ok(Self {
            jobs: Some(jobs),
            results,
            workers,
            pending: HashMap::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            clients: HashMap::new(),
            revision: 1,
            exhausted: false,
            cursor: 0,
        })
    }
    fn advance_revision(&mut self) -> bool {
        if let Some(next) = self.revision.checked_add(1) {
            self.revision = next;
            true
        } else {
            self.exhausted = true;
            for (_, cancelled) in self.pending.values() {
                cancelled.store(true, Ordering::Relaxed);
            }
            false
        }
    }
    #[cfg(test)]
    fn invalidate(&mut self) {
        self.advance_revision();
        for (_, cancelled) in self.pending.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
    /// Retire exactly the matching accepted job. Unrelated commits may advance
    /// the world counter while this tile's captured dependencies remain valid.
    fn finish(&mut self, result: &worker::Completion) -> bool {
        if self
            .pending
            .get(&result.key)
            .is_none_or(|(revision, _)| *revision != result.revision)
        {
            return false;
        }
        let (_, cancelled) = self.pending.remove(&result.key).unwrap();
        !cancelled.load(Ordering::Relaxed)
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        for (_, cancelled) in self.pending.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.jobs.take();
        // The completion channel can hold every accepted job; joining cannot
        // deadlock even if the coordinator stops draining at shutdown.
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
pub(super) fn handle(state: &mut State, id: u64, message: ClientMessage) -> io::Result<()> {
    let Some(client) = state.clients.get(&id) else {
        return Ok(());
    };
    match message {
        ClientMessage::LodConfig { horizon } => {
            let horizon = if horizon == 0 || state.lod.exhausted {
                0
            } else {
                horizon.clamp(
                    128,
                    if state.world.lod_max_level() == 4 {
                        1024
                    } else {
                        512
                    },
                )
            };
            state.lod.clients.insert(
                id,
                Interest {
                    horizon,
                    requests: VecDeque::new(),
                },
            );
            client.enqueue(ServerMessage::LodStatus {
                session: client.action_epoch,
                horizon,
                max_level: state.world.lod_max_level(),
            });
        }
        ClientMessage::LodRequest { request, key } => {
            let Some(interest) = state.lod.clients.get_mut(&id) else {
                return Ok(());
            };
            let permitted = key.bounds().is_some_and(|b| {
                let p = client.position();
                let h = f32::from(interest.horizon);
                key.level <= state.world.lod_max_level()
                    && interest.horizon != 0
                    && b[0] as f32 <= p[0] + h
                    && b[2] as f32 >= p[0] - h
                    && b[1] as f32 <= p[2] + h
                    && b[3] as f32 >= p[2] - h
            });
            if request == 0 || !permitted || interest.requests.len() >= MAX_CLIENT_REQUESTS {
                let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                    session: client.action_epoch,
                    request,
                    key,
                });
            } else if !interest.requests.iter().any(|r| r.id == request) {
                interest.requests.push_back(Request {
                    id: request,
                    key,
                    accepted: Instant::now(),
                });
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
/// Called after near publication, with bounded fair admission and delivery.
/// Never compete behind an already substantial gameplay backlog.
pub(super) fn poll(state: &mut State) {
    while let Ok(result) = state.lod.results.try_recv() {
        let valid = state.lod.finish(&result);
        tracing::debug!(target: "bloxgloom::lod_loading", key=?result.key, revision=result.revision, valid,
            available=result.tile.is_some(), error=result.error.as_deref(),
            worker_ms=crate::lod::loading::ms(result.elapsed), worker_queue_ms=crate::lod::loading::ms(result.queue_age),
            sources_ms=crate::lod::loading::ms(result.sources), cache_read_ms=crate::lod::loading::ms(result.cache_read),
            generation_ms=crate::lod::loading::ms(result.generation), cache_write_ms=crate::lod::loading::ms(result.cache_write),
            completion_wait_ms=crate::lod::loading::ms(result.finished.elapsed()), cache_hit=result.cache_hit, "LOD terrain completion");
        if !valid {
            continue;
        }
        if let Some(tile) = result.tile {
            if state.lod.cache.len() >= MAX_CACHE
                && let Some(old) = state.lod.order.pop_front()
            {
                state.lod.cache.remove(&old);
            }
            state.lod.order.push_back(tile.key);
            state.lod.cache.insert(tile.key, tile);
        } else {
            for (&id, interest) in &mut state.lod.clients {
                let Some(client) = state.clients.get(&id) else {
                    continue;
                };
                interest.requests.retain(|request| {
                    if request.key != result.key {
                        return true;
                    }
                    let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                        session: client.action_epoch,
                        request: request.id,
                        key: request.key,
                    });
                    false
                });
            }
        }
    }
    state
        .lod
        .clients
        .retain(|id, _| state.clients.contains_key(id));
    // Retire abandoned requests after movement. Cancellation is observed between
    // source chunks, so teleports do not keep expensive old terrain jobs alive.
    for (&id, interest) in &mut state.lod.clients {
        let Some(client) = state.clients.get(&id) else {
            continue;
        };
        let p = client.position();
        let h = f32::from(interest.horizon);
        interest.requests.retain(|request| {
            let active = request.key.bounds().is_some_and(|b| {
                b[0] as f32 <= p[0] + h
                    && b[2] as f32 >= p[0] - h
                    && b[1] as f32 <= p[2] + h
                    && b[3] as f32 >= p[2] - h
            });
            if !active {
                let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                    session: client.action_epoch,
                    request: request.id,
                    key: request.key,
                });
            }
            active
        });
    }
    for (key, (_, cancelled)) in &state.lod.pending {
        if !state
            .lod
            .clients
            .values()
            .any(|i| i.requests.iter().any(|request| &request.key == key))
        {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
    scheduling::poll(state);
}
/// Fence builds and ready summaries only where committed
/// chunks intersect their horizontal footprint. Huge publications fall back to
/// one explicit world-wide refresh instead of flooding control queues.
pub(super) fn invalidate(state: &mut State, chunks: impl IntoIterator<Item = ChunkKey>) {
    if !state.lod.advance_revision() {
        state.lod.cache.clear();
        state.lod.order.clear();
        for (&id, interest) in &mut state.lod.clients {
            interest.horizon = 0;
            interest.requests.clear();
            if let Some(client) = state.clients.get(&id) {
                client.enqueue(ServerMessage::LodStatus {
                    session: client.action_epoch,
                    horizon: 0,
                    max_level: state.world.lod_max_level(),
                });
            }
        }
        return;
    }
    let mut affected = std::collections::BTreeSet::new();
    for chunk in chunks {
        let Some(x) = chunk.x.checked_mul(crate::world::CHUNK_SIZE as i32) else {
            continue;
        };
        let Some(z) = chunk.z.checked_mul(crate::world::CHUNK_SIZE as i32) else {
            continue;
        };
        for level in 0..=4 {
            if let Some(key) = TileKey::containing(level, x, z) {
                affected.insert(key);
            }
        }
        if affected.len() > 32 {
            break;
        }
    }
    let all = affected.len() > 32;
    for (key, (_, cancelled)) in &state.lod.pending {
        if all || affected.contains(key) {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
    if all {
        state.lod.cache.clear();
        state.lod.order.clear();
    } else {
        state.lod.cache.retain(|key, _| !affected.contains(key));
        state.lod.order.retain(|key| !affected.contains(key));
    }
    for (&id, interest) in &state.lod.clients {
        if interest.horizon == 0 {
            continue;
        }
        let Some(client) = state.clients.get(&id) else {
            continue;
        };
        if all {
            client.enqueue(ServerMessage::LodInvalidateAll {
                session: client.action_epoch,
                revision: state.lod.revision,
            });
        } else {
            for &key in &affected {
                if !client.enqueue(ServerMessage::LodInvalidate {
                    session: client.action_epoch,
                    key,
                    revision: state.lod.revision,
                }) {
                    break;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests;
