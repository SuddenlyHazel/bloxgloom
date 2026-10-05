//! Dedicated bounded distant mesh work. Material sampling happens here as well.
use crate::{
    content::Catalog,
    lod::{LodTile, TileKey},
    render::lod::{self, Mesh},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

pub(super) struct Job {
    pub tile: Arc<LodTile>,
    pub neighbors: Vec<Arc<LodTile>>,
    pub generation: u64,
    pub submitted: Instant,
    pub trace: Option<crate::lod::loading::ClientTrace>,
}
pub(super) struct Result {
    pub generation: u64,
    pub key: TileKey,
    pub mesh: std::result::Result<Mesh, String>,
    pub queue_time: Duration,
    pub material_time: Duration,
    pub mesh_time: Duration,
    pub finished: Instant,
}
pub(super) struct Worker {
    jobs: Option<SyncSender<Job>>,
    pub results: Receiver<Result>,
    current: Arc<Mutex<HashMap<TileKey, u64>>>,
}
impl Worker {
    pub fn new(catalog: Arc<Catalog>) -> Self {
        Self::with_workers(
            catalog,
            crate::lod::loading::worker_count("BLOXGLOOM_LOD_MESH_WORKERS"),
        )
    }
    pub(super) fn with_workers(catalog: Arc<Catalog>, count: usize) -> Self {
        assert!((1..=4).contains(&count));
        let (jobs, receiver) = mpsc::sync_channel::<Job>(8);
        let (completed, results) = mpsc::sync_channel(8);
        let current = Arc::new(Mutex::new(HashMap::new()));
        let receiver = Arc::new(Mutex::new(receiver));
        let colors = Arc::new(OnceLock::new());
        // Share one lazy preparation across lanes, without keeping a retired
        // session's catalog alive while idle. Preparation stays on workers.
        let catalog = Arc::downgrade(&catalog);
        for lane in 0..count {
            let receiver = receiver.clone();
            let revisions = current.clone();
            let catalog = catalog.clone();
            let colors = colors.clone();
            let completed = completed.clone();
            thread::Builder::new()
                .name(format!("distant-mesh-{lane}"))
                .spawn(move || {
                    loop {
                        let received = receiver.lock().unwrap().recv();
                        let Ok(job) = received else {
                            break;
                        };
                        let valid = || {
                            revisions.lock().unwrap().get(&job.tile.key) == Some(&job.generation)
                        };
                        if !valid() {
                            continue;
                        }
                        let started = Instant::now();
                        let queue_time = started.saturating_duration_since(job.submitted);
                        let Some(catalog) = catalog.upgrade() else {
                            return;
                        };
                        let colors = colors.get_or_init(|| lod::FaceColors::new(&catalog));
                        let material_time = started.elapsed();
                        let meshing = Instant::now();
                        let neighbors: Vec<_> = job.neighbors.iter().map(AsRef::as_ref).collect();
                        let mesh =
                            lod::mesh(&job.tile, &neighbors, &catalog, colors).map(|mut mesh| {
                                mesh.loading = job.trace.map(|mut trace| {
                                    trace.queued = Instant::now();
                                    trace
                                });
                                mesh
                            });
                        let mesh_time = meshing.elapsed();
                        drop(catalog);
                        let mut result = Result {
                            key: job.tile.key,
                            generation: job.generation,
                            mesh,
                            queue_time,
                            material_time,
                            mesh_time,
                            finished: Instant::now(),
                        };
                        while valid() {
                            match completed.try_send(result) {
                                Ok(()) => break,
                                Err(TrySendError::Disconnected(_)) => return,
                                Err(TrySendError::Full(value)) => {
                                    result = value;
                                    thread::park_timeout(Duration::from_millis(5));
                                }
                            }
                        }
                    }
                })
                .expect("distant mesh worker");
        }
        tracing::info!(target: "bloxgloom::lod_loading", workers=count, queue=8, "LOD mesh pool ready");
        Self {
            jobs: Some(jobs),
            results,
            current,
        }
    }
    pub fn submit(&self, job: Job) -> bool {
        let key = job.tile.key;
        let generation = job.generation;
        let mut current = self.current.lock().unwrap();
        current.insert(key, generation);
        if self
            .jobs
            .as_ref()
            .is_some_and(|sender| sender.try_send(job).is_ok())
        {
            true
        } else {
            current.remove(&key);
            false
        }
    }
    pub fn cancel(&self, key: TileKey) {
        self.current.lock().unwrap().remove(&key);
    }
    pub fn clear(&self) {
        self.current.lock().unwrap().clear();
    }
    pub fn stop(&mut self) {
        self.clear();
        self.jobs = None;
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;
