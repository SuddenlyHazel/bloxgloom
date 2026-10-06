//! Revisioned scene assembly; the window thread only queues immutable chunks.
use super::scene::{Chunk, Scene};
use crate::world::ChunkKey;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};
struct ChunkUpdate {
    revision: u64,
    key: ChunkKey,
    chunk: Option<Arc<Chunk>>,
}
struct GpuContext {
    device: wgpu::Device,
    materials: wgpu::BindGroupLayout,
    size: wgpu::Extent3d,
}
enum Update {
    Chunk(ChunkUpdate),
    Configure { revision: u64, context: GpuContext },
}
pub(crate) struct Ready {
    pub revision: u64,
    pub scene: Scene,
    pub gpu: Option<super::gpu::Gpu>,
}
pub(crate) struct Worker {
    updates: mpsc::Sender<Update>,
    ready: Arc<Mutex<Option<Ready>>>,
    pub revision: u64,
    pub gpu_configured: bool,
    latest_revision: Arc<AtomicU64>,
}
impl Worker {
    pub fn new(storage_limit: u64) -> Self {
        let (updates, receive) = mpsc::channel::<Update>();
        let ready = Arc::new(Mutex::new(None));
        let published = ready.clone();
        let latest_revision = Arc::new(AtomicU64::new(0));
        let current_revision = latest_revision.clone();
        thread::Builder::new()
            .name("ray-scene".into())
            .spawn(move || {
                let mut chunks = BTreeMap::new();
                let mut context = None;
                while let Ok(update) = receive.recv() {
                    let mut revision = 0;
                    let apply = |chunks: &mut BTreeMap<ChunkKey, Arc<Chunk>>,
                                 context: &mut Option<GpuContext>,
                                 revision: &mut u64,
                                 update: Update| {
                        match update {
                            Update::Chunk(update) => {
                                *revision = update.revision;
                                if let Some(chunk) = update.chunk {
                                    chunks.insert(update.key, chunk);
                                } else {
                                    chunks.remove(&update.key);
                                }
                            }
                            Update::Configure {
                                revision: current,
                                context: configured,
                            } => {
                                *revision = current;
                                *context = Some(configured);
                            }
                        }
                    };
                    apply(&mut chunks, &mut context, &mut revision, update);
                    for update in receive.try_iter() {
                        apply(&mut chunks, &mut context, &mut revision, update);
                    }
                    // Reject oversized scenes before duplicating every chunk's
                    // triangle allocation or constructing a doomed BVH. An empty
                    // current revision deliberately selects the safe fallback.
                    let triangle_bytes = chunks.values().try_fold(0u64, |total, chunk| {
                        total.checked_add(chunk.byte_len() as u64)
                    });
                    let scene = if triangle_bytes.is_some_and(|bytes| bytes <= storage_limit) {
                        Scene::build(chunks.values().cloned())
                    } else {
                        tracing::warn!(
                            ?triangle_bytes,
                            storage_limit,
                            "ray triangles exceed storage limit; skipping scene build"
                        );
                        Scene::default()
                    };
                    // A rebuild may have become obsolete while assembling its
                    // BVH. Do not spend GPU allocation/compilation on that result.
                    if current_revision.load(Ordering::Acquire) != revision {
                        continue;
                    }
                    // Device objects are Send. Buffer initialization, shader and
                    // pipeline compilation run here rather than during redraw.
                    let gpu = context.as_ref().and_then(|context| {
                        (scene.fits(storage_limit) && !scene.nodes.is_empty()).then(|| {
                            super::gpu::Gpu::new(
                                &context.device,
                                &scene,
                                context.size,
                                &context.materials,
                            )
                        })
                    });
                    publish(
                        &published,
                        Ready {
                            revision,
                            scene,
                            gpu,
                        },
                    );
                }
            })
            .expect("ray scene worker starts");
        Self {
            updates,
            ready,
            revision: 0,
            gpu_configured: false,
            latest_revision,
        }
    }
    pub fn set(&mut self, key: ChunkKey, chunk: Option<Arc<Chunk>>) {
        self.revision = self.revision.wrapping_add(1);
        self.latest_revision.store(self.revision, Ordering::Release);
        let _ = self.updates.send(Update::Chunk(ChunkUpdate {
            revision: self.revision,
            key,
            chunk,
        }));
    }
    pub fn configure(
        &mut self,
        device: &wgpu::Device,
        materials: wgpu::BindGroupLayout,
        size: wgpu::Extent3d,
    ) {
        if self.gpu_configured {
            return;
        }
        self.gpu_configured = true;
        let _ = self.updates.send(Update::Configure {
            revision: self.revision,
            context: GpuContext {
                device: device.clone(),
                materials,
                size,
            },
        });
    }
    pub fn poll_ready(&self) -> Option<Ready> {
        let result = self
            .ready
            .lock()
            .expect("ray result mailbox is not poisoned")
            .take();
        result.filter(|r| r.revision == self.revision)
    }
    #[cfg(test)]
    pub fn poll(&self) -> Option<Scene> {
        self.poll_ready().map(|r| r.scene)
    }
}

// At most one complete scene waits for admission. The worker publishes in
// revision order; wrap-aware comparison also prevents stale deliveries from
// replacing a newer result. Never hold the lock while freeing a large scene.
fn publish(mailbox: &Mutex<Option<Ready>>, result: Ready) {
    let mut slot = mailbox.lock().expect("ray result mailbox is not poisoned");
    let replaced = if slot
        .as_ref()
        .is_none_or(|old| result.revision.wrapping_sub(old.revision) <= u64::MAX / 2)
    {
        slot.replace(result)
    } else {
        Some(result)
    };
    drop(slot);
    drop(replaced);
}

#[cfg(test)]
mod tests;
