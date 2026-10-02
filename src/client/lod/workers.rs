//! Dedicated bounded distant mesh work. Material sampling happens here as well.
use crate::{
    content::Catalog,
    lod::{LodTile, TileKey},
    render::lod::{self, Mesh},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
};

pub(super) struct Job {
    pub tile: Arc<LodTile>,
    pub neighbors: Vec<Arc<LodTile>>,
    pub generation: u64,
}
pub(super) struct Result {
    pub generation: u64,
    pub key: TileKey,
    pub mesh: std::result::Result<Mesh, String>,
}
pub(super) struct Worker {
    jobs: Option<SyncSender<Job>>,
    pub results: Receiver<Result>,
    current: Arc<Mutex<HashMap<TileKey, u64>>>,
}
impl Worker {
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let (jobs, receiver) = mpsc::sync_channel::<Job>(8);
        let (completed, results) = mpsc::sync_channel(8);
        let current = Arc::new(Mutex::new(HashMap::new()));
        let revisions = Arc::clone(&current);
        // An idle lane must not keep a retired session's catalog alive.
        // Prepare texture colors lazily, on the first actual mesh job.
        let catalog = Arc::downgrade(&catalog);
        thread::Builder::new()
            .name("distant-mesh".into())
            .spawn(move || {
                let mut colors = None;
                while let Ok(job) = receiver.recv() {
                    let valid =
                        || revisions.lock().unwrap().get(&job.tile.key) == Some(&job.generation);
                    if !valid() {
                        continue;
                    }
                    let Some(catalog) = catalog.upgrade() else {
                        return;
                    };
                    let colors = colors.get_or_insert_with(|| lod::FaceColors::new(&catalog));
                    let neighbors: Vec<_> = job.neighbors.iter().map(AsRef::as_ref).collect();
                    let mesh = lod::mesh(&job.tile, &neighbors, &catalog, colors);
                    drop(catalog);
                    let mut result = Result {
                        key: job.tile.key,
                        generation: job.generation,
                        mesh,
                    };
                    while valid() {
                        match completed.try_send(result) {
                            Ok(()) => break,
                            Err(TrySendError::Disconnected(_)) => return,
                            Err(TrySendError::Full(value)) => {
                                result = value;
                                thread::park_timeout(std::time::Duration::from_millis(5));
                            }
                        }
                    }
                }
            })
            .expect("distant mesh worker");
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
