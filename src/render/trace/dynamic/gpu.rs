//! Three storage bindings: current geometry, immutable assets, current poses/looks.
//! Static terrain admission and revisions are independent of this frame data.
use super::*;
use std::{cell::Cell, collections::HashMap, num::NonZeroU64};
use wgpu::util::DeviceExt;
const INSTANCE_WORDS: usize = 44;
const TRIANGLE_WORDS: usize = 36;
const NODE_WORDS: usize = 12;
pub(crate) struct DynamicGpu {
    pub layout: wgpu::BindGroupLayout,
    pub group: wgpu::BindGroup,
    compute_layout: wgpu::BindGroupLayout,
    compute: wgpu::BindGroup,
    deform: wgpu::ComputePipeline,
    refit: wgpu::ComputePipeline,
    geometry: wgpu::Buffer,
    source: wgpu::Buffer,
    frame: wgpu::Buffer,
    steps: wgpu::Buffer,
    assets: Vec<Arc<DynamicAsset>>,
    offsets: HashMap<usize, u32>,
    uploader: assets::Uploader,
    levels: u32,
    triangles: u32,
    nodes: u32,
    generation: u64,
    encoded_generation: Cell<u64>,
    topology: Vec<usize>,
    previous_frame: Vec<u32>,
    lighting_changed: bool,
    history_changed: bool,
    previous_appearance: Vec<u32>,
}
fn buffer(
    device: &wgpu::Device,
    label: &str,
    words: &[u32],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(if words.is_empty() { &[0u32] } else { words }),
        usage,
    })
}
impl DynamicGpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dynamic ray targets"),
            entries: &[storage(0, true), storage(1, true), storage(2, true)],
        });
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dynamic ray deformation and refit"),
            entries: &[
                storage(0, false),
                storage(1, true),
                storage(2, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("exact dynamic ray poses"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}\n{}",
                    DEFORMATION_SHADER,
                    include_str!("buffers.wgsl")
                        .replace("@group(2)", "@group(0)")
                        .replace(
                            "var<storage,read> dyn_geometry",
                            "var<storage,read_write> dyn_geometry"
                        ),
                    include_str!("deform.wgsl")
                )
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dynamic ray compute"),
            bind_group_layouts: &[Some(&compute_layout)],
            immediate_size: 0,
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let geometry = buffer(
            device,
            "empty dynamic rays",
            &[0; 16],
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let source = buffer(
            device,
            "empty dynamic assets",
            &[0; 16],
            wgpu::BufferUsages::STORAGE,
        );
        let frame = buffer(
            device,
            "empty dynamic frame",
            &[0; 16],
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let mut step_words = vec![0; 64 * 64];
        for level in 0..64 {
            step_words[level * 64] = level as u32;
        }
        let steps = buffer(
            device,
            "dynamic refit levels",
            &step_words,
            wgpu::BufferUsages::UNIFORM,
        );
        let group = Self::bind(device, &layout, &geometry, &source, &frame, None);
        let compute = Self::bind(
            device,
            &compute_layout,
            &geometry,
            &source,
            &frame,
            Some(&steps),
        );
        Self {
            layout,
            group,
            compute_layout,
            compute,
            deform: pipeline("dynamic_deform"),
            refit: pipeline("dynamic_refit"),
            geometry,
            source,
            frame,
            steps,
            assets: Vec::new(),
            offsets: HashMap::new(),
            uploader: assets::Uploader::new(device.clone()),
            levels: 0,
            triangles: 0,
            nodes: 0,
            generation: 0,
            encoded_generation: Cell::new(0),
            topology: Vec::new(),
            previous_frame: Vec::new(),
            lighting_changed: false,
            history_changed: false,
            previous_appearance: Vec::new(),
        }
    }
    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        geometry: &wgpu::Buffer,
        source: &wgpu::Buffer,
        frame: &wgpu::Buffer,
        steps: Option<&wgpu::Buffer>,
    ) -> wgpu::BindGroup {
        let mut entries = vec![(0, geometry), (1, source), (2, frame)]
            .into_iter()
            .map(|(binding, b)| wgpu::BindGroupEntry {
                binding,
                resource: b.as_entire_binding(),
            })
            .collect::<Vec<_>>();
        if let Some(b) = steps {
            entries.push(wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: b,
                    offset: 0,
                    size: NonZeroU64::new(16),
                }),
            });
        }
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("current dynamic targets"),
            layout,
            entries: &entries,
        })
    }
    /// All posed targets replace the prior frame, including empty/removal frames.
    /// Returns false while native uploads are pending or exceed adapter limits;
    /// callers keep voxel/SSR fallback, never stale dynamic poses.
    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        targets: &DynamicTargets,
    ) -> bool {
        let limit = device
            .limits()
            .max_storage_buffer_binding_size
            .min(device.limits().max_buffer_size);
        let mut source_changed = false;
        if let Some(ready) = self.uploader.poll()
            && let Some(source) = ready.source
        {
            self.source = source;
            self.offsets = ready.offsets;
            self.assets = ready.assets;
            source_changed = true;
        }
        if targets
            .instances
            .iter()
            .any(|i| !self.offsets.contains_key(&(Arc::as_ptr(&i.asset) as usize)))
        {
            self.uploader
                .request(targets.instances.iter().map(|i| i.asset.clone()));
            queue.write_buffer(&self.geometry, 0, bytemuck::cast_slice(&[0u32; 8]));
            self.topology.clear();
            self.previous_frame.clear();
            self.triangles = 0;
            self.nodes = 0;
            self.lighting_changed = true;
            self.history_changed = true;
            return false;
        }
        let Some(packet) = frame::pack(targets, &self.offsets, &self.topology, limit) else {
            tracing::warn!("dynamic ray targets exceed adapter limits; current targets disabled");
            queue.write_buffer(&self.geometry, 0, bytemuck::cast_slice(&[0u32; 8]));
            self.triangles = 0;
            self.nodes = 0;
            self.topology.clear();
            self.previous_frame.clear();
            self.lighting_changed = true;
            self.history_changed = true;
            return false;
        };
        let frame::Frame {
            words: frame,
            header: geometry,
            nodes: topology_nodes,
            topology,
            levels,
            triangles: triangle_count,
            node_count,
            node_base,
            topology_changed,
        } = packet;
        let required_geometry_bytes = (node_base + node_count * NODE_WORDS) as u64 * 4;
        if self.geometry.size() < required_geometry_bytes {
            self.geometry = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("posed dynamic geometry"),
                size: required_geometry_bytes,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
        }
        if topology_changed {
            queue.write_buffer(&self.geometry, 0, bytemuck::cast_slice(&geometry));
            if !topology_nodes.is_empty() {
                queue.write_buffer(
                    &self.geometry,
                    node_base as u64 * 4,
                    bytemuck::cast_slice(&topology_nodes),
                );
            }
        }
        if self.frame.size() < frame.len() as u64 * 4 {
            self.frame = buffer(
                device,
                "current dynamic poses",
                &frame,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            );
        } else {
            queue.write_buffer(&self.frame, 0, bytemuck::cast_slice(&frame));
        }
        self.group = Self::bind(
            device,
            &self.layout,
            &self.geometry,
            &self.source,
            &self.frame,
            None,
        );
        self.compute = Self::bind(
            device,
            &self.compute_layout,
            &self.geometry,
            &self.source,
            &self.frame,
            Some(&self.steps),
        );
        self.levels = levels;
        self.triangles = triangle_count as u32;
        self.nodes = node_count as u32;
        let mut appearance = Vec::new();
        for index in 0..targets.instances.len() {
            let at = 8 + index * INSTANCE_WORDS;
            appearance.extend_from_slice(&frame[at + 28..at + 36]);
            appearance.extend([frame[at + 7], frame[at + 40]]);
            let parts = frame[at + 6] as usize;
            let count = targets.instances[index].parts.len() * 8;
            appearance.extend_from_slice(&frame[parts..parts + count]);
        }
        self.history_changed =
            appearance != self.previous_appearance || topology_changed || source_changed;
        self.previous_appearance = appearance;
        self.lighting_changed = frame != self.previous_frame || topology_changed || source_changed;
        if self.lighting_changed {
            self.generation = self.generation.wrapping_add(1);
        }
        self.previous_frame = frame;
        self.topology = topology;
        true
    }
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.encoded_generation.get() == self.generation {
            return;
        }
        if self.triangles == 0 {
            self.encoded_generation.set(self.generation);
            return;
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("exact current actor/drop triangles"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.deform);
            pass.set_bind_group(0, &self.compute, &[0]);
            pass.dispatch_workgroups(self.triangles.div_ceil(64), 1, 1);
        }
        for level in (0..self.levels).rev() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("dynamic BLAS/TLAS refit level"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.refit);
            pass.set_bind_group(0, &self.compute, &[level * 256]);
            pass.dispatch_workgroups(self.nodes.div_ceil(64), 1, 1);
        }
        self.encoded_generation.set(self.generation);
    }
    /// Mirrors the current uploaded geometry header, including disabled/pending frames.
    pub(in crate::render::trace) fn is_empty(&self) -> bool {
        self.nodes == 0
    }

    pub fn byte_len(&self) -> usize {
        self.geometry.size() as usize + self.source.size() as usize + self.frame.size() as usize
    }

    /// Includes world/rig transforms, appearance/alpha choices, pose, motion,
    /// item animation, admission and removal. Equal packets reuse posed geometry.
    /// Appearance/admission changes remain conservative; exact moving poses
    /// contribute through the current paired correction, not static history.
    pub fn history_changed(&self) -> bool {
        self.history_changed && self.lighting_changed()
    }

    pub fn lighting_changed(&self) -> bool {
        self.lighting_changed || self.encoded_generation.get() != self.generation
    }
}

#[cfg(test)]
mod tests;

mod assets;

mod frame;
