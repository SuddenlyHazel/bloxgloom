use super::{
    super::{Camera, daylight::Atmosphere},
    Mesh,
};
use crate::{lod::TileKey, world::ChunkKey};
use glam::Vec3;
use std::collections::{HashMap, HashSet, VecDeque};
use wgpu::util::DeviceExt;
// Independent hard residency cap also accommodates the 1,024-block fixture.
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_PENDING: usize = 8;
#[derive(Debug)]
pub(crate) enum UploadError {
    QueueFull(Box<Mesh>),
    Budget(Box<Mesh>),
}
const COVERAGE_SLOTS: usize = super::near_coverage::SLOTS;
struct Tile {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    count: u32,
    water_index: wgpu::Buffer,
    water_count: u32,
    bytes: usize,
    revision: u64,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    coverage: Box<super::coverage::Coverage>,
    bounds: Option<[Vec3; 2]>,
    distance: f32,
}
pub(crate) struct Gpu {
    pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    materials: wgpu::BindGroup,
    options: wgpu::Buffer,
    textured: bool,
    camera: wgpu::Buffer,
    group: wgpu::BindGroup,
    tile_layout: wgpu::BindGroupLayout,
    coverage: wgpu::Buffer,
    tiles: HashMap<TileKey, Tile>,
    pending: VecDeque<Mesh>,
    selected: Vec<TileKey>,
    visible: Vec<TileKey>,
    selection_dirty: bool,
    near: super::near_coverage::NearCoverage,
    horizon: u16,
    jitter: glam::Vec2,
}
impl Gpu {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        material_pipeline: &wgpu::RenderPipeline,
        materials: &wgpu::BindGroup,
    ) -> Self {
        let coverage = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ready 3D chunk coverage"),
            size: (COVERAGE_SLOTS * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let p = super::pipelines::new(device, format, &coverage, material_pipeline);
        Self {
            pipeline: p.opaque,
            water_pipeline: p.water,
            materials: materials.clone(),
            options: p.options,
            textured: std::env::var("BLOXGLOOM_LOD_TEXTURES").as_deref() != Ok("0"),
            camera: p.camera,
            group: p.group,
            tile_layout: p.tile_layout,
            coverage,
            tiles: HashMap::new(),
            pending: VecDeque::new(),
            selected: vec![],
            visible: vec![],
            selection_dirty: true,
            near: super::near_coverage::NearCoverage::default(),
            horizon: 512,
            jitter: glam::Vec2::ZERO,
        }
    }
    pub(crate) fn enqueue(&mut self, mut mesh: Mesh) -> Result<(), UploadError> {
        if mesh.byte_len() > MAX_BYTES {
            return Err(UploadError::Budget(Box::new(mesh)));
        }
        if !self.pending.iter().any(|m| m.key == mesh.key) && self.pending.len() >= MAX_PENDING {
            return Err(UploadError::QueueFull(Box::new(mesh)));
        }
        if self
            .pending
            .iter()
            .any(|m| m.key == mesh.key && m.revision > mesh.revision)
            || self
                .tiles
                .get(&mesh.key)
                .is_some_and(|t| t.revision > mesh.revision)
        {
            return Ok(());
        }
        // Reserve positive replacement growth before admitting work. A full
        // resident budget rejects immediately rather than stranding the upload
        // queue behind one tile; existing drawable coverage stays installed.
        let resident: usize = self.tiles.values().map(|t| t.bytes).sum();
        let reserved: usize = self
            .pending
            .iter()
            .filter(|m| m.key != mesh.key)
            .map(|m| {
                m.byte_len()
                    .saturating_sub(self.tiles.get(&m.key).map_or(0, |t| t.bytes))
            })
            .sum();
        let growth = mesh
            .byte_len()
            .saturating_sub(self.tiles.get(&mesh.key).map_or(0, |t| t.bytes));
        let mut keys: HashSet<_> = self
            .tiles
            .keys()
            .copied()
            .chain(self.pending.iter().map(|m| m.key))
            .collect();
        keys.insert(mesh.key);
        if resident + reserved + growth > MAX_BYTES || keys.len() > 256 {
            return Err(UploadError::Budget(Box::new(mesh)));
        }
        self.pending.retain(|m| m.key != mesh.key);
        if let Some(trace) = &mut mesh.loading {
            trace.queued = std::time::Instant::now();
        }
        self.pending.push_back(mesh);
        Ok(())
    }
    /// One bounded upload per frame, after near mesh uploads.
    pub(crate) fn upload(&mut self, device: &wgpu::Device) -> usize {
        let Some(m) = self.pending.front() else {
            return 0;
        };
        let current: usize = self.tiles.values().map(|t| t.bytes).sum();
        let old = self.tiles.get(&m.key).map_or(0, |t| t.bytes);
        if current - old + m.byte_len() > MAX_BYTES
            || (!self.tiles.contains_key(&m.key) && self.tiles.len() >= 256)
        {
            return 0;
        }
        let m = self.pending.pop_front().unwrap();
        let upload_started = std::time::Instant::now();
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD relative tile origin"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("LOD tile"),
            layout: &self.tile_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let buffer = |bytes: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("LOD geometry"),
                contents: if bytes.is_empty() { &[0; 4] } else { bytes },
                usage,
            })
        };
        self.tiles.insert(
            m.key,
            Tile {
                vertex: buffer(
                    bytemuck::cast_slice(&m.vertices),
                    wgpu::BufferUsages::VERTEX,
                ),
                index: buffer(bytemuck::cast_slice(&m.indices), wgpu::BufferUsages::INDEX),
                count: m.indices.len() as u32,
                water_index: buffer(
                    bytemuck::cast_slice(&m.water_indices),
                    wgpu::BufferUsages::INDEX,
                ),
                water_count: m.water_indices.len() as u32,
                bytes: m.byte_len(),
                revision: m.revision,
                uniform,
                group,
                coverage: m.coverage,
                bounds: m.bounds,
                distance: 0.0,
            },
        );
        if let Some(trace) = m.loading {
            tracing::debug!(target: "bloxgloom::lod_loading", key=?m.key, request=trace.request, revision=m.revision,
                upload_wait_ms=crate::lod::loading::ms(upload_started.saturating_duration_since(trace.queued)),
                upload_cpu_ms=crate::lod::loading::ms(upload_started.elapsed()),
                request_to_upload_ms=crate::lod::loading::ms(trace.requested.elapsed()), "LOD GPU upload");
        }
        self.selection_dirty = true;
        1
    }
    pub(crate) fn set_horizon(&mut self, horizon: u16) {
        self.horizon = horizon;
    }
    pub(crate) fn discard_obsolete(&mut self, key: TileKey, minimum: u64) {
        self.pending
            .retain(|m| m.key != key || m.revision >= minimum);
    }
    pub(crate) fn remove(&mut self, key: TileKey) {
        self.selection_dirty |= self.tiles.remove(&key).is_some();
        self.pending.retain(|m| m.key != key);
    }
    pub(crate) fn clear(&mut self) {
        self.tiles.clear();
        self.pending.clear();
        self.selected.clear();
        self.visible.clear();
        self.selection_dirty = true;
    }
    pub(crate) fn resident_bytes(&self) -> usize {
        self.tiles.values().map(|t| t.bytes).sum()
    }
    pub(crate) fn selected_count(&self) -> usize {
        self.visible.len()
    }
    pub(crate) fn ready_keys(&self) -> impl Iterator<Item = TileKey> + '_ {
        self.tiles.keys().copied()
    }
    pub(crate) fn set_jitter(&mut self, jitter: glam::Vec2) {
        self.jitter = jitter;
    }
    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        camera: Camera,
        width: u32,
        height: u32,
        atmosphere: Atmosphere,
        near: impl Iterator<Item = ChunkKey>,
    ) {
        if self.horizon == 0 {
            self.visible.clear();
            return;
        }
        let relative = Camera {
            position: Vec3::ZERO,
            ..camera
        };
        let vp = super::super::visibility::view_projection(relative, width, height);
        let vp = super::super::post::temporal::jitter_matrix(vp, self.jitter, width, height);
        let mut data = atmosphere.camera_data(vp, Vec3::ZERO);
        if self.horizon > 0 {
            data[28] = f32::from(self.horizon) * 0.65;
            data[29] = f32::from(self.horizon);
        }
        queue.write_buffer(&self.camera, 0, bytemuck::cast_slice(&data));
        queue.write_buffer(
            &self.options,
            0,
            bytemuck::cast_slice(&[
                super::super::water::time(),
                if self.textured { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ]),
        );
        if let Some(slots) = self.near.update(near) {
            queue.write_buffer(&self.coverage, 0, bytemuck::cast_slice(slots));
        }
        if self.selection_dirty {
            self.selected =
                select_ready(self.tiles.keys().copied().collect(), |parent, children| {
                    super::coverage::can_refine(
                        &self.tiles[&parent].coverage,
                        children.map(|k| self.tiles[&k].coverage.as_ref()),
                    )
                });
            self.selection_dirty = false;
        }
        self.visible.clear();
        for key in &self.selected {
            let Some([x, z, _, _]) = key.bounds() else {
                continue;
            };
            let origin = Vec3::new(x as f32, 0.0, z as f32) - camera.position;
            let tile = self.tiles.get_mut(key).unwrap();
            tile.distance = tile.bounds.map_or(origin.length_squared(), |[min, max]| {
                (origin + (min + max) * 0.5).length_squared()
            });
            if !tile.bounds.is_some_and(|[min, max]| {
                super::super::visibility::bounds_visible(vp, min + origin, max + origin)
            }) {
                continue;
            }
            self.visible.push(*key);
            let mut bytes = [0u8; 32];
            bytes[..16].copy_from_slice(bytemuck::cast_slice(&[
                origin.x,
                origin.y,
                origin.z,
                key.sample_width().unwrap_or(1) as f32,
            ]));
            bytes[16..].copy_from_slice(bytemuck::cast_slice(&[x, 0, z, 0i32]));
            queue.write_buffer(&tile.uniform, 0, &bytes);
        }
    }
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        let mut triangles = 0;
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_bind_group(2, &self.materials, &[]);
        for key in &self.visible {
            let tile = &self.tiles[key];
            pass.set_bind_group(1, &tile.group, &[]);
            pass.set_vertex_buffer(0, tile.vertex.slice(..));
            pass.set_index_buffer(tile.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..tile.count, 0, 0..1);
            triangles += tile.count as usize / 3;
        }
        triangles
    }
    pub(crate) fn draw_water(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        pass.set_pipeline(&self.water_pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_bind_group(2, &self.materials, &[]);
        let mut visible = self.visible.clone();
        visible.sort_by(|a, b| {
            // Camera-relative origins are available through preparation; sort
            // stored distances rather than mapping/readback of GPU uniforms.
            self.tiles[b]
                .distance
                .total_cmp(&self.tiles[a].distance)
                .then_with(|| a.cmp(b))
        });
        let mut triangles = 0;
        for key in visible {
            let tile = &self.tiles[&key];
            if tile.water_count == 0 {
                continue;
            }
            pass.set_bind_group(1, &tile.group, &[]);
            pass.set_vertex_buffer(0, tile.vertex.slice(..));
            pass.set_index_buffer(tile.water_index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..tile.water_count, 0, 0..1);
            triangles += tile.water_count as usize / 3;
        }
        triangles
    }
    /// Offscreen callers share the same post-AO transparent ordering as play.
    pub(crate) fn draw_water_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        indirect: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> usize {
        let mut attachments =
            super::super::scene_ao::attachments(scene, indirect, wgpu::Color::TRANSPARENT);
        for attachment in attachments.iter_mut().flatten() {
            attachment.ops.load = wgpu::LoadOp::Load;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("distant water after ambient resolve"),
            color_attachments: &attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        self.draw_water(&mut pass)
    }
}
/// A ready parent covers its entire footprint until all four children can draw.
pub(super) fn select_ready(
    ready: HashSet<TileKey>,
    can_refine: impl Fn(TileKey, [TileKey; 4]) -> bool,
) -> Vec<TileKey> {
    fn visit(
        k: TileKey,
        ready: &HashSet<TileKey>,
        out: &mut Vec<TileKey>,
        can_refine: &impl Fn(TileKey, [TileKey; 4]) -> bool,
    ) {
        if let Some(children) = k.children()
            && children.iter().all(|c| ready.contains(c))
            && can_refine(k, children)
        {
            for c in children {
                visit(c, ready, out, can_refine);
            }
            return;
        }
        out.push(k);
    }
    let mut roots: Vec<_> = ready
        .iter()
        .copied()
        .filter(|k| {
            let mut p = k.parent();
            while let Some(key) = p {
                if ready.contains(&key) {
                    return false;
                }
                p = key.parent();
            }
            true
        })
        .collect();
    roots.sort_by_key(|k| (k.level, k.x, k.z));
    let mut out = vec![];
    for k in roots {
        visit(k, &ready, &mut out, &can_refine);
    }
    out
}
