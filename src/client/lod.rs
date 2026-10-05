//! Session-owned distant terrain replicas. Never read by gameplay or collision.
#[cfg(test)]
mod tests;
mod uploads;
mod workers;

use crate::{
    content::Catalog,
    lod::{LodTile, TileKey},
    protocol::ClientMessage,
    render::{Renderer, lod::desired_tiles},
};
use glam::Vec3;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use workers::{Job, Worker};

const MAX_TILES: usize = 128;
const MAX_REQUESTS: usize = 4;
const RETRY: Duration = Duration::from_secs(10);

struct Request {
    id: u64,
    started: Instant,
}

pub(super) struct State {
    session: u64,
    configured: Option<u16>,
    pub(super) horizon: u16,
    next_request: u64,
    next_build: u64,
    quality: u8,
    max_level: u8,
    center: Option<[i32; 2]>,
    wanted: Vec<TileKey>,
    tiles: HashMap<TileKey, Arc<LodTile>>,
    timings: HashMap<TileKey, crate::lod::loading::ClientTrace>,
    requests: HashMap<TileKey, Request>,
    retry: HashMap<TileKey, Instant>,
    minimum: HashMap<TileKey, u64>,
    mesh_retry: HashMap<TileKey, Instant>,
    pending_mesh: HashSet<TileKey>,
    builds: HashMap<TileKey, u64>,
    worker: Worker,
    ready_upload: Option<uploads::Pending>,
}

impl State {
    pub(super) fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            session: 0,
            configured: None,
            horizon: 0,
            next_request: 1,
            next_build: 1,
            quality: 1,
            max_level: 4,
            center: None,
            wanted: vec![],
            tiles: HashMap::new(),
            timings: HashMap::new(),
            requests: HashMap::new(),
            retry: HashMap::new(),
            minimum: HashMap::new(),
            mesh_retry: HashMap::new(),
            pending_mesh: HashSet::new(),
            builds: HashMap::new(),
            worker: Worker::new(catalog),
            ready_upload: None,
        }
    }
    pub(super) fn status(&mut self, session: u64, horizon: u16, max_level: u8) {
        if session != 0 && session == self.session {
            self.horizon = horizon.min(self.configured.unwrap_or(0));
            self.max_level = max_level.clamp(3, 4);
            self.center = None;
        }
    }
    pub(super) fn accept(&mut self, session: u64, request: u64, tile: LodTile) {
        let key = tile.key;
        if session != self.session || self.requests.get(&key).is_none_or(|r| r.id != request) {
            return;
        }
        let requested = self.requests.remove(&key).unwrap().started;
        let received = Instant::now();
        tracing::debug!(target: "bloxgloom::lod_loading", ?key, request, revision=tile.revision,
            request_roundtrip_ms=crate::lod::loading::ms(received.saturating_duration_since(requested)), "LOD client received");
        if tile.revision < self.minimum.get(&key).copied().unwrap_or(0) {
            return;
        }
        if !self.wanted.contains(&key)
            || (!self.tiles.contains_key(&key) && self.tiles.len() == MAX_TILES)
        {
            return;
        }
        self.timings.insert(
            key,
            crate::lod::loading::ClientTrace {
                request,
                requested,
                received,
                queued: received,
            },
        );
        self.tiles.insert(key, Arc::new(tile));
        self.retry.remove(&key);
        self.rebuild_neighbors(key);
    }
    pub(super) fn unavailable(&mut self, session: u64, request: u64, key: TileKey) {
        if session == self.session && self.requests.get(&key).is_some_and(|r| r.id == request) {
            self.requests.remove(&key);
            self.retry.insert(key, Instant::now() + RETRY);
        }
    }
    pub(super) fn invalidate(&mut self, session: u64, key: TileKey, revision: u64) {
        if session != self.session || !self.wanted.contains(&key) {
            return;
        }
        let minimum = self.minimum.entry(key).or_default();
        if revision <= *minimum {
            return;
        }
        *minimum = revision;
        self.requests.remove(&key);
        self.retry.remove(&key);
        self.builds.remove(&key);
        self.pending_mesh.remove(&key);
        self.worker.cancel(key);
        // Keep displayed geometry until its authoritative replacement is ready.
    }
    pub(super) fn invalidate_all(&mut self, session: u64, revision: u64) {
        if session != self.session {
            return;
        }
        for key in self.wanted.clone() {
            self.invalidate(session, key, revision);
        }
    }
    fn rebuild_neighbors(&mut self, key: TileKey) {
        for other in self
            .tiles
            .keys()
            .copied()
            .filter(|other| neighboring(key, *other))
        {
            self.builds.remove(&other);
            self.worker.cancel(other);
            self.mesh_retry.remove(&other);
            self.pending_mesh.insert(other);
        }
    }
    pub(super) fn reset(&mut self) {
        self.worker.clear();
        self.ready_upload = None;
        self.tiles.clear();
        self.timings.clear();
        self.requests.clear();
        self.retry.clear();
        self.minimum.clear();
        self.mesh_retry.clear();
        self.pending_mesh.clear();
        self.builds.clear();
        self.wanted.clear();
        self.center = None;
    }
    pub(super) fn retire(&mut self) {
        self.reset();
        self.horizon = 0;
        self.configured = None;
        self.session = 0;
        self.worker.stop();
    }
    pub(super) fn cached_tiles(&self) -> usize {
        self.tiles.len()
    }
    pub(super) fn update(
        &mut self,
        position: Vec3,
        horizon: u16,
        quality: u8,
        session: u64,
        renderer: &mut Renderer,
        mut send: impl FnMut(ClientMessage) -> bool,
    ) {
        if session == 0 {
            return;
        }
        if self.session != session {
            self.reset();
            renderer.clear_lod();
            self.session = session;
            self.configured = None;
        }
        if self.configured != Some(horizon) && send(ClientMessage::LodConfig { horizon }) {
            self.configured = Some(horizon);
            self.horizon = 0;
            self.reset();
            renderer.clear_lod();
        }
        renderer.set_lod_horizon(self.horizon);
        for (&key, &minimum) in &self.minimum {
            renderer.discard_obsolete_lod(key, minimum);
        }
        let center = [
            (position.x.floor() as i32).div_euclid(32),
            (position.z.floor() as i32).div_euclid(32),
        ];
        let moved = self.center.is_none_or(|old| {
            let x = f64::from(position.x) - f64::from(old[0]) * 32.0;
            let z = f64::from(position.z) - f64::from(old[1]) * 32.0;
            !(-8.0..40.0).contains(&x) || !(-8.0..40.0).contains(&z)
        });
        if moved || self.quality != quality {
            self.center = Some(center);
            self.quality = quality;
            self.wanted = desired_tiles(position, self.horizon, quality, self.max_level);
            self.wanted.truncate(MAX_TILES);
            let wanted: HashSet<_> = self.wanted.iter().copied().collect();
            let removed: Vec<_> = self
                .tiles
                .keys()
                .copied()
                .filter(|k| !wanted.contains(k))
                .collect();
            for key in removed {
                self.tiles.remove(&key);
                self.timings.remove(&key);
                self.worker.cancel(key);
                renderer.remove_lod_tile(key);
            }
            self.requests.retain(|key, _| wanted.contains(key));
            self.retry.retain(|key, _| wanted.contains(key));
            self.minimum.retain(|key, _| wanted.contains(key));
            self.mesh_retry.retain(|key, _| wanted.contains(key));
            self.pending_mesh.retain(|key| wanted.contains(key));
            self.builds.retain(|key, _| wanted.contains(key));
        }
        let upload_ready = uploads::retry(self, |mesh| renderer.enqueue_lod_mesh(mesh));
        for _ in 0..if upload_ready { 8 } else { 0 } {
            let Ok(result) = self.worker.results.try_recv() else {
                break;
            };
            let key = result.key;
            if self.builds.get(&key) != Some(&result.generation) {
                continue;
            }
            tracing::debug!(target: "bloxgloom::lod_loading", ?key, generation=result.generation,
                mesh_queue_ms=crate::lod::loading::ms(result.queue_time), material_prepare_ms=crate::lod::loading::ms(result.material_time),
                meshing_ms=crate::lod::loading::ms(result.mesh_time), result_wait_ms=crate::lod::loading::ms(result.finished.elapsed()),
                "LOD mesh completion");
            match result.mesh {
                Ok(mesh) => {
                    if mesh.revision < self.minimum.get(&key).copied().unwrap_or(0) {
                        self.builds.remove(&key);
                        continue;
                    }
                    if !uploads::offer(self, result.generation, mesh, |mesh| {
                        renderer.enqueue_lod_mesh(mesh)
                    }) {
                        break;
                    }
                }
                Err(error) => {
                    self.builds.remove(&key);
                    tracing::warn!(?key, %error, "distant mesh unavailable within resource budget");
                    self.mesh_retry
                        .insert(key, Instant::now() + Duration::from_secs(60));
                    self.pending_mesh.insert(key);
                }
            }
        }

        for _ in 0..if self.ready_upload.is_none() { 4 } else { 0 } {
            let Some(key) = self
                .wanted
                .iter()
                .find(|key| {
                    self.pending_mesh.contains(key)
                        && self
                            .mesh_retry
                            .get(key)
                            .is_none_or(|until| *until <= Instant::now())
                })
                .copied()
            else {
                break;
            };
            let Some(tile) = self.tiles.get(&key) else {
                self.pending_mesh.remove(&key);
                continue;
            };
            if tile.revision < self.minimum.get(&key).copied().unwrap_or(0) {
                self.pending_mesh.remove(&key);
                continue;
            }
            let generation = self.next_build;
            self.next_build = self.next_build.wrapping_add(1).max(1);
            let neighbors = self
                .tiles
                .iter()
                .filter(|(other, _)| **other != key && neighboring(key, **other))
                .map(|(_, tile)| Arc::clone(tile))
                .collect();
            let submitted = Instant::now();
            let trace = self.timings.get(&key).copied();
            if !self.worker.submit(Job {
                tile: Arc::clone(tile),
                neighbors,
                generation,
                submitted,
                trace,
            }) {
                break;
            }
            tracing::debug!(target: "bloxgloom::lod_loading", ?key, generation,
                client_schedule_ms=trace.map(|t| crate::lod::loading::ms(submitted.saturating_duration_since(t.received))), "LOD mesh admitted");
            self.builds.insert(key, generation);
            self.pending_mesh.remove(&key);
        }
        let now = Instant::now();
        self.requests
            .retain(|_, request| now.duration_since(request.started) < Duration::from_secs(30));
        for key in &self.wanted {
            if self.requests.len() >= MAX_REQUESTS {
                break;
            }
            if self.requests.contains_key(key)
                || self.retry.get(key).is_some_and(|until| *until > now)
            {
                continue;
            }
            if self
                .tiles
                .get(key)
                .is_some_and(|tile| tile.revision >= self.minimum.get(key).copied().unwrap_or(0))
            {
                continue;
            }
            let request = self.next_request;
            if !send(ClientMessage::LodRequest { request, key: *key }) {
                break;
            }
            self.next_request = self.next_request.wrapping_add(1).max(1);
            self.requests.insert(
                *key,
                Request {
                    id: request,
                    started: now,
                },
            );
        }
    }
}

fn neighboring(a: TileKey, b: TileKey) -> bool {
    if a == b {
        return true;
    }
    let (Some(a), Some(b)) = (a.bounds(), b.bounds()) else {
        return false;
    };
    ((a[2] == b[0] || b[2] == a[0]) && a[1] < b[3] && b[1] < a[3])
        || ((a[3] == b[1] || b[3] == a[1]) && a[0] < b[2] && b[0] < a[2])
}
