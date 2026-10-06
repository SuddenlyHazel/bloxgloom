//! Scene-space light transport independent of screen visibility.
mod deformation;
pub(crate) mod dynamic;
mod gpu;
mod optimizations;
pub(crate) mod profiling;
pub(crate) mod scene;
#[cfg(test)]
mod tests;
mod water_filter_diagnostics;
mod worker;
use crate::render::daylight::Atmosphere;
use crate::world::ChunkKey;
use glam::{Mat4, Vec3};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) struct TraceLighting {
    worker: worker::Worker,
    scene: Option<scene::Scene>,
    lod_pages: Vec<scene::Scene>,
    lod_targets: BTreeMap<crate::lod::TileKey, (u64, Arc<scene::Chunk>)>,
    dynamic_ready: bool,
    dynamic_targets: dynamic::DynamicTargets,
    eye_water: bool,
    water_time: f32,
    gpu: Option<gpu::Gpu>,
    active_revision: u64,
    enabled: bool,
    storage_limit: u64,
    profile: Option<profiling::Profile>,
    headless: bool,
    size: Option<wgpu::Extent3d>,
    water_reconstruction_supported: bool,
}
impl TraceLighting {
    pub fn new(device: &wgpu::Device) -> Self {
        let enabled = device.limits().max_storage_buffers_per_shader_stage >= 12
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
            lod_pages: Vec::new(),
            lod_targets: BTreeMap::new(),
            dynamic_ready: true,
            dynamic_targets: Default::default(),
            eye_water: false,
            water_time: 0.0,
            gpu: None,
            active_revision: 0,
            enabled,
            storage_limit,
            profile: None,
            headless: false,
            size: None,
            water_reconstruction_supported: false,
        }
    }
    /// Freeze adapter admission before worker or headless GPU creation. The
    /// default has no extra fragment storage binding or reconstruction images.
    pub fn set_water_reconstruction_adapter(&mut self, adapter: &wgpu::Adapter) {
        if self.gpu.is_some() || self.worker.gpu_configured {
            return;
        }
        self.water_reconstruction_supported = gpu::Gpu::water_reconstruction_supported(adapter);
        if self.enabled
            && gpu::Gpu::water_reconstruction_requested()
            && !self.water_reconstruction_supported
        {
            tracing::warn!(
                "adapter lacks first-water reconstruction support; using split-only path"
            );
        }
    }
    pub fn set(&mut self, key: ChunkKey, chunk: Option<Arc<scene::Chunk>>) {
        if self.enabled {
            self.worker.set(key, chunk);
        }
    }
    pub fn sync_lod_targets(
        &mut self,
        targets: Vec<(crate::lod::TileKey, u64, Arc<scene::Chunk>)>,
    ) {
        if !self.enabled {
            return;
        }
        let next: BTreeMap<_, _> = targets
            .into_iter()
            .map(|(key, revision, chunk)| (key, (revision, chunk)))
            .collect();
        for key in self
            .lod_targets
            .keys()
            .filter(|key| !next.contains_key(key))
        {
            self.worker.set_lod(*key, None);
        }
        for (key, (revision, chunk)) in &next {
            if self
                .lod_targets
                .get(key)
                .is_none_or(|(old, source)| old != revision || !Arc::ptr_eq(source, chunk))
            {
                self.worker.set_lod(*key, Some(chunk.clone()));
            }
        }
        self.lod_targets = next;
    }
    pub fn prepare_dynamic(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        targets: &dynamic::DynamicTargets,
    ) -> bool {
        if !self.enabled {
            return true;
        }
        self.dynamic_targets = targets.clone();
        self.dynamic_ready = if let Some(gpu) = &mut self.gpu {
            gpu.prepare_dynamic(device, queue, targets)
        } else {
            targets.instances.is_empty()
        };
        self.dynamic_ready
    }
    pub fn set_water_time(&mut self, water_time: f32) {
        self.water_time = water_time;
    }
    pub fn set_eye_water(&mut self, wet: bool) {
        self.eye_water = wet;
    }
    /// Synchronous preparation for headless fixtures, already off the window thread.
    pub fn prepare_scene_with_lod(
        &mut self,
        chunks: impl IntoIterator<Item = Arc<scene::Chunk>>,
        lod: Vec<Arc<scene::Chunk>>,
    ) {
        if !self.enabled {
            return;
        }
        self.headless = true;
        let chunks: Vec<_> = chunks.into_iter().collect();
        let Some((scene, lod_pages)) = worker::assemble_sources(&chunks, &lod, self.storage_limit)
        else {
            self.scene = None;
            self.gpu = None;
            self.lod_pages.clear();
            return;
        };
        self.scene = Some(scene);
        self.lod_pages = lod_pages;
        self.gpu = None;
        self.active_revision = self.worker.revision;
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
            self.lod_pages = ready.lod_pages;
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
            && self.dynamic_ready
            && self.scene.as_ref().is_some_and(|s| {
                !s.nodes.is_empty()
                    || s.coverage.get(7).is_some_and(|cells| *cells > 0)
                    || self.lod_pages.iter().any(|page| !page.nodes.is_empty())
            })
            && (self.headless || self.gpu.is_some())
    }
    pub fn scene_bytes(&self) -> usize {
        self.scene.as_ref().map_or(0, scene::Scene::byte_len)
            + self
                .lod_pages
                .iter()
                .map(scene::Scene::byte_len)
                .sum::<usize>()
            + self.gpu.as_ref().map_or(0, gpu::Gpu::dynamic_bytes)
    }
    pub fn water_history_diagnostics(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        self.gpu
            .as_ref()
            .map(|gpu| gpu.water_history_diagnostics(device, queue))
            .transpose()
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
            && self.headless
            && self.gpu.is_none()
            && size.width > 0
            && size.height > 0
            && let Some(scene) = &self.scene
        {
            self.gpu = Some(gpu::Gpu::new_with_lod_supported(
                device,
                scene,
                &self.lod_pages,
                size,
                &material_pipeline.get_bind_group_layout(1),
                self.water_reconstruction_supported,
            ));
        }
        if self.enabled
            && !self.headless
            && !self.worker.gpu_configured
            && size.width > 0
            && size.height > 0
        {
            self.size = Some(size);
            self.worker.configure(
                device,
                material_pipeline.get_bind_group_layout(1),
                size,
                self.water_reconstruction_supported,
            );
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
            self.gpu = Some(gpu::Gpu::new_with_lod_supported(
                device,
                self.scene.as_ref().unwrap(),
                &self.lod_pages,
                size,
                &layout,
                self.water_reconstruction_supported,
            ));
            if !self
                .gpu
                .as_mut()
                .unwrap()
                .prepare_dynamic(device, queue, &self.dynamic_targets)
            {
                self.dynamic_ready = false;
                return;
            }
        }
        self.gpu.as_mut().unwrap().eye_water = self.eye_water;
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
            self.water_time,
            profile.as_ref(),
        );
    }

    /// Headless diagnostics only; ordinary live frames never request GPU waits.
    pub(crate) fn set_headless_transport_scheduling(
        &mut self,
        edge: Option<u32>,
        inflight: u32,
    ) -> Result<(), String> {
        self.gpu
            .as_mut()
            .ok_or("headless ray GPU is not prepared")?
            .set_headless_transport_scheduling(edge, inflight)
    }

    pub(crate) fn take_submission_error(&mut self) -> Option<String> {
        self.gpu.as_mut().and_then(gpu::Gpu::take_scheduling_error)
    }
}
