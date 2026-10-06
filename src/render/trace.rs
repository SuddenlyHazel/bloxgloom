//! Scene-space light transport independent of screen visibility.
mod deformation;
mod gpu;
mod optimizations;
pub(crate) mod profiling;
pub(crate) mod scene;
#[cfg(test)]
mod tests;
mod worker;
use crate::render::daylight::Atmosphere;
use crate::world::ChunkKey;
use glam::{Mat4, Vec3};
use std::sync::Arc;
pub(crate) struct TraceLighting {
    worker: worker::Worker,
    scene: Option<scene::Scene>,
    gpu: Option<gpu::Gpu>,
    active_revision: u64,
    enabled: bool,
    storage_limit: u64,
    profile: Option<profiling::Profile>,
    headless: bool,
    size: Option<wgpu::Extent3d>,
}
impl TraceLighting {
    pub fn new(device: &wgpu::Device) -> Self {
        let enabled = device.limits().max_storage_buffers_per_shader_stage >= 5
            && !super::bsl_reference::enabled()
            && super::post::temporal::supported(device)
            && std::env::var("BLOXGLOOM_GI").is_ok_and(|v| v == "1");
        let storage_limit = device
            .limits()
            .max_storage_buffer_binding_size
            .min(device.limits().max_buffer_size);
        Self {
            worker: worker::Worker::new(storage_limit),
            scene: None,
            gpu: None,
            active_revision: 0,
            enabled,
            storage_limit,
            profile: None,
            headless: false,
            size: None,
        }
    }
    pub fn set(&mut self, key: ChunkKey, chunk: Option<Arc<scene::Chunk>>) {
        if self.enabled {
            self.worker.set(key, chunk);
        }
    }
    /// Synchronous preparation for headless fixtures, already off the window thread.
    pub fn prepare_scene(&mut self, chunks: impl IntoIterator<Item = Arc<scene::Chunk>>) {
        if self.enabled {
            self.headless = true;
            let chunks: Vec<_> = chunks.into_iter().collect();
            if chunks.iter().map(|c| c.byte_len()).sum::<usize>() as u64 > self.storage_limit {
                self.scene = None;
                self.gpu = None;
                return;
            }
            let scene = scene::Scene::build(chunks);
            self.scene = scene.fits(self.storage_limit).then_some(scene);
            self.gpu = None;
            self.active_revision = self.worker.revision;
        }
    }
    pub fn install(&mut self, device: &wgpu::Device) {
        if let Some(ready) = self.worker.poll_ready() {
            let scene = ready.scene;
            if !scene.fits(self.storage_limit) {
                tracing::warn!(
                    bytes = scene.byte_len(),
                    "ray scene exceeds adapter storage limits; using voxel/SSR fallback"
                );
                self.scene = None;
                self.gpu = None;
                return;
            }
            self.gpu = ready.gpu;
            if let (Some(gpu), Some(size)) = (&mut self.gpu, self.size) {
                gpu.ensure_size(device, size);
            }
            self.scene = Some(scene);
            self.active_revision = self.worker.revision;
        }
    }
    pub fn ready(&self) -> bool {
        self.enabled
            && self.active_revision == self.worker.revision
            && self.scene.as_ref().is_some_and(|s| !s.nodes.is_empty())
            && (self.headless || self.gpu.is_some())
    }
    pub fn scene_bytes(&self) -> usize {
        self.scene.as_ref().map_or(0, scene::Scene::byte_len)
    }
    /// Queue preparation before frame readiness is evaluated. Device buffer
    /// initialization and pipeline compilation remain on the scene worker.
    pub fn prepare_gpu(
        &mut self,
        device: &wgpu::Device,
        material_pipeline: &wgpu::RenderPipeline,
        size: wgpu::Extent3d,
    ) {
        if self.enabled
            && !self.headless
            && !self.worker.gpu_configured
            && size.width > 0
            && size.height > 0
        {
            self.size = Some(size);
            self.worker
                .configure(device, material_pipeline.get_bind_group_layout(1), size);
        }
    }
    /// Optional diagnostics enabled only by an explicit headless harness call.
    pub fn profile(
        &mut self,
        device: &wgpu::Device,
        frames: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.enabled {
            return Err("trace profiling requires enabled scene transport (set BLOXGLOOM_GI=1 on a supported adapter and disable the BSL reference branch)".into());
        }
        self.profile = Some(profiling::Profile::new(device, frames)?);
        Ok(())
    }
    pub fn profile_active(&mut self, active: bool) {
        if let Some(profile) = &mut self.profile {
            profile.activate(active);
        }
    }
    pub fn profile_results(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Option<profiling::Samples>, Box<dyn std::error::Error>> {
        self.profile
            .as_ref()
            .map(|profile| profile.read(device, queue))
            .transpose()
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: wgpu::Extent3d) {
        self.size = Some(size);
        if let Some(gpu) = &mut self.gpu {
            gpu.resize(device, size);
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        normal: &wgpu::TextureView,
        response: &wgpu::TextureView,
        indirect: &wgpu::TextureView,
        material_pipeline: &wgpu::RenderPipeline,
        materials: &wgpu::BindGroup,
        matrix: Mat4,
        eye: Vec3,
        atmosphere: Atmosphere,
    ) {
        if !self.ready() {
            return;
        }
        if self.gpu.is_none() {
            let size = scene.texture().size();
            let layout = material_pipeline.get_bind_group_layout(1);
            self.gpu = Some(gpu::Gpu::new(
                device,
                self.scene.as_ref().unwrap(),
                size,
                &layout,
            ));
        }
        let profile = self.profile.as_mut().and_then(profiling::Profile::frame);
        self.gpu.as_mut().unwrap().resolve(
            device,
            queue,
            encoder,
            scene,
            depth,
            normal,
            response,
            indirect,
            materials,
            matrix,
            eye,
            atmosphere,
            profile.as_ref(),
        );
    }
}
