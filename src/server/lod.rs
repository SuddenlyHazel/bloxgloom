//! Independent bounded distant interest, generation, revision fencing, and delivery.
mod worker;
use super::{ClientMessage, ServerMessage, State};
use crate::lod::{LodTile, TileKey};
use crate::world::{ChunkKey, World};
use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
const MAX_PENDING: usize = 4;
const MAX_CLIENT_REQUESTS: usize = 8;
const MAX_CACHE: usize = 64;
#[derive(Default)]
struct Interest {
    horizon: u16,
    requests: VecDeque<(u64, TileKey)>,
}
pub(super) struct Service {
    jobs: Option<SyncSender<worker::Job>>,
    results: Receiver<worker::Completion>,
    worker: Option<JoinHandle<()>>,
    pending: HashMap<TileKey, (u64, Arc<AtomicBool>)>,
    cache: HashMap<TileKey, LodTile>,
    order: VecDeque<TileKey>,
    clients: HashMap<u64, Interest>,
    revision: u64,
    cursor: u64,
}
impl Service {
    pub(super) fn new(world: &World, root: std::path::PathBuf) -> io::Result<Self> {
        let (jobs, receiver) = mpsc::sync_channel(MAX_PENDING);
        let (sender, results) = mpsc::sync_channel(MAX_PENDING);
        let world = world.loader_view();
        let worker = thread::Builder::new()
            .name("lod-terrain".into())
            .spawn(move || worker::run(world, root, receiver, sender))?;
        Ok(Self {
            jobs: Some(jobs),
            results,
            worker: Some(worker),
            pending: HashMap::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            clients: HashMap::new(),
            revision: 1,
            cursor: 0,
        })
    }
    pub(super) fn invalidate(&mut self) {
        self.revision = self.revision.saturating_add(1);
        for (_, cancelled) in self.pending.values() {
            cancelled.store(true, Ordering::Relaxed);
        }
        // Cached old geometry remains on clients until a ready replacement.
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
        if let Some(worker) = self.worker.take() {
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
            let horizon = if horizon == 0 {
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
            } else if !interest.requests.iter().any(|(r, _)| *r == request) {
                interest.requests.push_back((request, key));
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
/// Called after near publication: at most one distant frame per client and four
/// globally per tick, and never behind an already substantial gameplay backlog.
pub(super) fn poll(state: &mut State) {
    while let Ok(result) = state.lod.results.try_recv() {
        state.lod.pending.remove(&result.key);
        tracing::debug!(key=?result.key,generation_ms=result.elapsed.as_millis(),queue_ms=result.queue_age.as_millis(),"LOD terrain completion");
        if result.revision != state.lod.revision {
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
                interest.requests.retain(|(request, key)| {
                    if *key != result.key {
                        return true;
                    }
                    let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                        session: client.action_epoch,
                        request: *request,
                        key: *key,
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
        interest.requests.retain(|(request, key)| {
            let active = key.bounds().is_some_and(|b| {
                b[0] as f32 <= p[0] + h
                    && b[2] as f32 >= p[0] - h
                    && b[1] as f32 <= p[2] + h
                    && b[3] as f32 >= p[2] - h
            });
            if !active {
                let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                    session: client.action_epoch,
                    request: *request,
                    key: *key,
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
            .any(|i| i.requests.iter().any(|(_, requested)| requested == key))
        {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
    let mut ids: Vec<_> = state.lod.clients.keys().copied().collect();
    ids.sort_unstable();
    let pivot = ids.partition_point(|id| *id <= state.lod.cursor);
    ids.rotate_left(pivot);
    let mut admitted = 0;
    for id in ids {
        let Some(client) = state.clients.get(&id) else {
            continue;
        };
        if client.sender.snapshot().queued_bytes > 32 * 1024
            || client.sender.snapshot().queued_frames > 8
        {
            continue;
        }
        let interest = state.lod.clients.get_mut(&id).unwrap();
        let Some(&(request, key)) = interest.requests.front() else {
            continue;
        };
        if let Some(tile) = state.lod.cache.get(&key) {
            if client
                .sender
                .try_send(ServerMessage::LodTile {
                    session: client.action_epoch,
                    request,
                    tile: tile.clone(),
                })
                .is_ok()
            {
                interest.requests.pop_front();
                state.lod.cursor = id;
                admitted += 1;
            }
            if admitted >= 4 {
                break;
            }
            continue;
        }
        if state.lod.pending.contains_key(&key) || state.lod.pending.len() >= MAX_PENDING {
            continue;
        }
        let Ok(overlays) = state.world.lod_overlays(key.bounds().unwrap()) else {
            interest.requests.pop_front();
            let _ = client.sender.try_send(ServerMessage::LodUnavailable {
                session: client.action_epoch,
                request,
                key,
            });
            continue;
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let revision = state.lod.revision;
        let children = key.children().and_then(|keys| {
            let values: Vec<_> = keys
                .iter()
                .map(|key| state.lod.cache.get(key).cloned())
                .collect::<Option<Vec<_>>>()?;
            values.try_into().ok()
        });
        let job = worker::Job {
            key,
            revision,
            overlays,
            children,
            requested_at: Instant::now(),
            cancelled: cancelled.clone(),
        };
        if state.lod.jobs.as_ref().unwrap().try_send(job).is_ok() {
            state.lod.pending.insert(key, (revision, cancelled));
            state.lod.cursor = id;
        }
    }
}
/// Fence builds globally, but invalidate ready summaries only where committed
/// chunks intersect their horizontal footprint. Huge publications fall back to
/// one explicit world-wide refresh instead of flooding control queues.
pub(super) fn invalidate(state: &mut State, chunks: impl IntoIterator<Item = ChunkKey>) {
    state.lod.invalidate();
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
