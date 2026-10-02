use super::{
    super::{Camera, daylight::Atmosphere},
    Mesh,
};
use crate::{lod::TileKey, world::ChunkKey};
use glam::Vec3;
use std::collections::{HashMap, HashSet, VecDeque};
use wgpu::util::DeviceExt;
// Measured 512-block fixture retains ~57 MiB of cave/terrain surfaces.
// Independent hard residency cap also accommodates the 1,024-block fixture.
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_PENDING: usize = 8;
const COVERAGE_SLOTS: usize = 16384;
struct Tile {
    vertex: wgpu::Buffer,
    index: wgpu::Buffer,
    count: u32,
    bytes: usize,
    revision: u64,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    coverage: super::coverage::Coverage,
}
pub(crate) struct Gpu {
    pipeline: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    group: wgpu::BindGroup,
    tile_layout: wgpu::BindGroupLayout,
    coverage: wgpu::Buffer,
    tiles: HashMap<TileKey, Tile>,
    pending: VecDeque<Mesh>,
    selected: Vec<TileKey>,
    horizon: u16,
}
impl Gpu {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("LOD camera"),
            size: 128,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let coverage = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ready 3D chunk coverage"),
            size: (COVERAGE_SLOTS * 16) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LOD camera coverage"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("LOD scene"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: coverage.as_entire_binding(),
                },
            ],
        });
        let tile_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("LOD tile origin"),
            entries: &[entry(0, wgpu::BufferBindingType::Uniform)],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("LOD shader"),
            source: wgpu::ShaderSource::Wgsl(
                super::super::fog::shader(include_str!("shader.wgsl")).into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("LOD pipeline"),
            bind_group_layouts: &[Some(&layout), Some(&tile_layout)],
            immediate_size: 0,
        });
        let attrs = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x3,3=>Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("distant terrain"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 44,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: super::super::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            camera,
            group,
            tile_layout,
            coverage,
            tiles: HashMap::new(),
            pending: VecDeque::new(),
            selected: vec![],
            horizon: 512,
        }
    }
    pub(crate) fn enqueue(&mut self, mesh: Mesh) -> Result<(), Mesh> {
        if mesh.byte_len() > MAX_BYTES
            || (!self.pending.iter().any(|m| m.key == mesh.key)
                && self.pending.len() >= MAX_PENDING)
        {
            return Err(mesh);
        }
        if self
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
            return Err(mesh);
        }
        self.pending.retain(|m| m.key != mesh.key);
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
                bytes: m.byte_len(),
                revision: m.revision,
                uniform,
                group,
                coverage: m.coverage,
            },
        );
        1
    }
    pub(crate) fn set_horizon(&mut self, horizon: u16) {
        self.horizon = horizon;
    }
    pub(crate) fn remove(&mut self, key: TileKey) {
        self.tiles.remove(&key);
        self.pending.retain(|m| m.key != key);
    }
    pub(crate) fn clear(&mut self) {
        self.tiles.clear();
        self.pending.clear();
        self.selected.clear();
    }
    pub(crate) fn ready_keys(&self) -> impl Iterator<Item = TileKey> + '_ {
        self.tiles.keys().copied()
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
            self.selected.clear();
            return;
        }
        let relative = Camera {
            position: Vec3::ZERO,
            ..camera
        };
        let vp = super::super::visibility::view_projection(relative, width, height);
        let mut data = atmosphere.camera_data(vp, Vec3::ZERO);
        if self.horizon > 0 {
            data[28] = f32::from(self.horizon) * 0.65;
            data[29] = f32::from(self.horizon);
        }
        queue.write_buffer(&self.camera, 0, bytemuck::cast_slice(&data));
        let mut slots = vec![[0i32; 4]; COVERAGE_SLOTS];
        for k in near {
            let mut index = hash(k.x, k.y, k.z);
            for _ in 0..COVERAGE_SLOTS {
                if slots[index][3] == 0 {
                    slots[index] = [k.x, k.y, k.z, 1];
                    break;
                }
                index = (index + 1) & (COVERAGE_SLOTS - 1);
            }
        }
        queue.write_buffer(&self.coverage, 0, bytemuck::cast_slice(&slots));
        self.selected = select_ready(self.tiles.keys().copied().collect(), |parent, children| {
            super::coverage::can_refine(
                &self.tiles[&parent].coverage,
                children.map(|k| &self.tiles[&k].coverage),
            )
        });
        if self.horizon == 0 {
            self.selected.clear();
        }
        for key in &self.selected {
            let Some([x, z, _, _]) = key.bounds() else {
                continue;
            };
            let origin = Vec3::new(x as f32, 0.0, z as f32) - camera.position;
            let mut bytes = Vec::with_capacity(32);
            bytes.extend_from_slice(bytemuck::cast_slice(&[origin.x, origin.y, origin.z, 0.0]));
            bytes.extend_from_slice(bytemuck::cast_slice(&[x, 0, z, 0i32]));
            queue.write_buffer(&self.tiles[key].uniform, 0, &bytes);
        }
    }
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> usize {
        let mut triangles = 0;
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        for key in &self.selected {
            let tile = &self.tiles[key];
            pass.set_bind_group(1, &tile.group, &[]);
            pass.set_vertex_buffer(0, tile.vertex.slice(..));
            pass.set_index_buffer(tile.index.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..tile.count, 0, 0..1);
            triangles += tile.count as usize / 3;
        }
        triangles
    }
}
fn entry(binding: u32, ty: wgpu::BufferBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
fn hash(x: i32, y: i32, z: i32) -> usize {
    ((x as u32).wrapping_mul(73856093)
        ^ (y as u32).wrapping_mul(19349663)
        ^ (z as u32).wrapping_mul(83492791)) as usize
        & (COVERAGE_SLOTS - 1)
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
