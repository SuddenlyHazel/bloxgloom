//! Revisioned scene assembly; the window thread only queues immutable chunks.
use super::scene::{Chunk, Scene};
use crate::{lod::TileKey, world::ChunkKey};
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
struct LodUpdate {
    revision: u64,
    key: TileKey,
    chunk: Option<Arc<Chunk>>,
}
struct GpuContext {
    device: wgpu::Device,
    materials: wgpu::BindGroupLayout,
    size: wgpu::Extent3d,
    water_reconstruction_supported: bool,
}
enum Update {
    Chunk(ChunkUpdate),
    Lod(LodUpdate),
    Configure { revision: u64, context: GpuContext },
}
pub(crate) struct Ready {
    pub revision: u64,
    pub scene: Scene,
    pub lod_pages: Vec<Scene>,
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
                let mut lod = BTreeMap::new();
                let mut context = None;
                while let Ok(update) = receive.recv() {
                    let mut revision = 0;
                    let apply = |chunks: &mut BTreeMap<ChunkKey, Arc<Chunk>>,
                                 lod: &mut BTreeMap<TileKey, Arc<Chunk>>,
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
                            Update::Lod(update) => {
                                *revision = update.revision;
                                if let Some(chunk) = update.chunk {
                                    lod.insert(update.key, chunk);
                                } else {
                                    lod.remove(&update.key);
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
                    apply(&mut chunks, &mut lod, &mut context, &mut revision, update);
                    for update in receive.try_iter() {
                        apply(&mut chunks, &mut lod, &mut context, &mut revision, update);
                    }
                    // Reject the entire revision if any geometry or medium
                    // binding cannot fit. Never publish partial occluders.
                    let assembled = assemble(&chunks, &lod, storage_limit);
                    let valid = assembled.is_some();
                    let (scene, lod_pages) = assembled.unwrap_or_default();
                    // A rebuild may have become obsolete while assembling its
                    // BVH. Do not spend GPU allocation/compilation on that result.
                    if current_revision.load(Ordering::Acquire) != revision {
                        continue;
                    }
                    // Device objects are Send. Buffer initialization, shader and
                    // pipeline compilation run here rather than during redraw.
                    let gpu = context.as_ref().and_then(|context| {
                        (valid && eligible(&scene, &lod_pages)).then(|| {
                            super::gpu::Gpu::new_with_lod_supported(
                                &context.device,
                                &scene,
                                &lod_pages,
                                context.size,
                                &context.materials,
                                context.water_reconstruction_supported,
                            )
                        })
                    });
                    publish(
                        &published,
                        Ready {
                            revision,
                            scene,
                            lod_pages,
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
    pub fn set_lod(&mut self, key: TileKey, chunk: Option<Arc<Chunk>>) {
        self.revision = self.revision.wrapping_add(1);
        self.latest_revision.store(self.revision, Ordering::Release);
        let _ = self.updates.send(Update::Lod(LodUpdate {
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
        water_reconstruction_supported: bool,
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
                water_reconstruction_supported,
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

// All assembly runs on the worker. Preflight immutable source payloads before
// duplicating BVHs, and medium records before extending their storage binding.
fn assemble(
    near: &BTreeMap<ChunkKey, Arc<Chunk>>,
    lod: &BTreeMap<TileKey, Arc<Chunk>>,
    limit: u64,
) -> Option<(Scene, Vec<Scene>)> {
    assemble_sources(
        &near.values().cloned().collect::<Vec<_>>(),
        &lod.values().cloned().collect::<Vec<_>>(),
        limit,
    )
}

pub(super) fn assemble_sources(
    near: &[Arc<Chunk>],
    lod: &[Arc<Chunk>],
    limit: u64,
) -> Option<(Scene, Vec<Scene>)> {
    let near_bytes = near
        .iter()
        .try_fold(0u64, |bytes, c| bytes.checked_add(c.byte_len() as u64))?;
    if near_bytes > limit {
        return None;
    }
    let pages = super::scene::pages::build(lod.iter().cloned(), limit)?;
    let mut scene = Scene::build(near.iter().cloned());
    let mut medium_bytes = 0u64;
    let mut has_medium = false;
    for c in near {
        if let Some(water) = c.water.as_ref().filter(|_| c.key.is_some()) {
            has_medium = true;
            medium_bytes = medium_bytes
                .checked_add(16)?
                .checked_add((water.mask.len() as u64).checked_mul(4)?)?;
        }
    }
    for c in lod {
        if let Some(water) = &c.coarse_water {
            has_medium = true;
            medium_bytes = medium_bytes.checked_add(water.byte_len() as u64)?;
        }
    }
    if has_medium {
        let total = (scene.coverage.len() as u64)
            .checked_mul(4)?
            .checked_add(16)?
            .checked_add(medium_bytes)?;
        if total > limit {
            return None;
        }
    }
    super::scene::volume::append_limited(&mut scene, near, lod, limit);
    if !scene.fits(limit)
        || pages.iter().any(|p| {
            let bytes = (p.nodes.len() as u64) * std::mem::size_of::<super::scene::Node>() as u64
                + (p.triangles.len() as u64) * std::mem::size_of::<super::scene::Triangle>() as u64;
            bytes.checked_add(16).is_none_or(|n| n > limit)
        })
    {
        return None;
    }
    Some((scene, pages))
}

fn eligible(scene: &Scene, pages: &[Scene]) -> bool {
    !scene.nodes.is_empty()
        || scene.coverage.get(7).is_some_and(|cells| *cells > 0)
        || pages.iter().any(|page| !page.nodes.is_empty())
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
