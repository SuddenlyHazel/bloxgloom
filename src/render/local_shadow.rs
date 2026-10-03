//! Real, bounded point-light occlusion: six perspective depth faces per source.
//! Maps are refreshed as whole lights, never a mixture of old/new cube faces.
use super::{DEPTH_FORMAT, sun_shadow};
use glam::{Mat4, Vec3, camera::rh};
use wgpu::util::DeviceExt;
mod draw;
#[cfg(test)]
mod gpu_history_tests;
#[cfg(test)]
pub(in crate::render) mod gpu_tests;
mod integration;
mod selection;
mod settings;
#[cfg(test)]
mod tests;
use selection::Slot;
pub use settings::Settings;

pub(crate) const MAX_SOURCES: usize = 4;
pub(crate) const FACES: usize = 6;
pub(crate) const UNIFORM_BYTES: u64 = 16 + MAX_SOURCES as u64 * 416;
const NEAR: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Source {
    pub position: Vec3,
    pub range: f32,
    pub color: [f32; 3],
}

pub(crate) struct Face {
    pub view: wgpu::TextureView,
    pub caster_group: wgpu::BindGroup,
    pub matrix: Mat4,
    uniform: wgpu::Buffer,
}
impl Face {
    pub(crate) fn contains_chunk(&self, key: crate::world::ChunkKey, padding: f32) -> bool {
        super::visibility::chunk_visible_padded(self.matrix, key, padding)
    }
    pub(crate) fn begin<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("local point light depth face"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        })
    }
}

pub(crate) struct LocalShadows {
    pub view: wgpu::TextureView,
    pub uniform: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    pub settings: Settings,
    faces: Vec<Face>,
    slots: Vec<Slot>,
    updates: Vec<usize>,
    cursor: usize,
}
impl LocalShadows {
    pub(crate) fn new(device: &wgpu::Device, camera: &wgpu::Buffer) -> Self {
        Self::new_with_settings(
            device,
            camera,
            Settings::configured(device.limits().max_texture_dimension_2d),
        )
    }

    pub(crate) fn new_with_settings(
        device: &wgpu::Device,
        camera: &wgpu::Buffer,
        settings: Settings,
    ) -> Self {
        let settings = settings.sanitized(device.limits().max_texture_dimension_2d);
        let texture = depth(
            device,
            settings.resolution,
            settings.count.max(1) as u32 * 6,
        );
        let view = array_view(&texture);
        let uniform = zero_uniform(device, UNIFORM_BYTES, "point light receiver uniform");
        let sampler = comparison_sampler(device);
        let dummy_array = array_view(&depth(device, 1, 6));
        let dummy_sun = depth(device, 1, 1).create_view(&Default::default());
        let dummy_uniform = zero_uniform(device, UNIFORM_BYTES, "disabled point receiver");
        let faces = (0..settings.count * 6)
            .map(|layer| {
                let view = texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer as u32,
                    array_layer_count: Some(1),
                    ..Default::default()
                });
                let uniform =
                    zero_uniform(device, sun_shadow::UNIFORM_BYTES, "point caster projection");
                let caster_group = sun_shadow::group_with_local(
                    device,
                    camera,
                    &uniform,
                    &dummy_sun,
                    &sampler,
                    &dummy_uniform,
                    &dummy_array,
                    &sampler,
                );
                Face {
                    view,
                    caster_group,
                    matrix: Mat4::IDENTITY,
                    uniform,
                }
            })
            .collect();
        Self {
            view,
            uniform,
            sampler,
            settings,
            faces,
            slots: vec![Slot::default(); settings.count],
            updates: Vec::new(),
            cursor: 0,
        }
    }

    pub(crate) fn update(
        &mut self,
        queue: &wgpu::Queue,
        eye: Vec3,
        sources: &[Source],
        dt_seconds: f32,
    ) {
        selection::select(&mut self.slots, eye, sources, self.settings, dt_seconds);
        self.updates.clear();
        // Round-robin bounds every active light's actor/terrain staleness to
        // ceil(count / updates) frames. New admissions are invisible until drawn.
        let count = self.slots.len();
        let mut refreshed = 0;
        for offset in 0..count {
            let index = (self.cursor + offset) % count;
            let Some(source) = self.slots[index].source else {
                continue;
            };
            for (face_index, matrix) in
                matrices(source.position, source.range.min(self.settings.range))
                    .into_iter()
                    .enumerate()
            {
                let layer = index * 6 + face_index;
                let face = &mut self.faces[layer];
                face.matrix = matrix;
                let mut data = [0.0f32; 24];
                data[..16].copy_from_slice(&matrix.to_cols_array());
                data[18] = -1.0; // Marks local caster for emissive source exclusion.
                data[20..23].copy_from_slice(&source.position.to_array());
                queue.write_buffer(&face.uniform, 0, bytemuck::cast_slice(&data));
                self.updates.push(layer);
            }
            self.slots[index].initialized = true;
            refreshed += 1;
            if refreshed == self.settings.updates {
                self.cursor = (index + 1) % count;
                break;
            }
        }
        let mut data = [0.0f32; UNIFORM_BYTES as usize / 4];
        data[..4].copy_from_slice(&[
            count as f32,
            1.0 / self.settings.resolution as f32,
            NEAR,
            self.settings.range,
        ]);
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(source) = slot.source else {
                continue;
            };
            let offset = 4 + index * 104;
            data[offset..offset + 3].copy_from_slice(&source.position.to_array());
            data[offset + 3] = source.range;
            data[offset + 4..offset + 7].copy_from_slice(&source.color);
            data[offset + 7] = if slot.initialized { slot.weight } else { 0.0 };
            for face in 0..6 {
                data[offset + 8 + face * 16..offset + 24 + face * 16]
                    .copy_from_slice(&self.faces[index * 6 + face].matrix.to_cols_array());
            }
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&data));
    }

    pub(crate) fn faces_to_update(&self) -> impl Iterator<Item = usize> + '_ {
        self.updates.iter().copied()
    }
    pub(crate) fn face(&self, index: usize) -> &Face {
        &self.faces[index]
    }
}

pub(crate) fn matrices(position: Vec3, range: f32) -> [Mat4; FACES] {
    let projection = rh::proj::directx::perspective(
        std::f32::consts::FRAC_PI_2,
        1.0,
        NEAR,
        range.max(NEAR * 2.0),
    );
    // +X,-X,+Y,-Y,+Z,-Z, matching dominant-axis receiver selection.
    [
        (Vec3::X, Vec3::Y),
        (-Vec3::X, Vec3::Y),
        (Vec3::Y, Vec3::Z),
        (-Vec3::Y, -Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (-Vec3::Z, Vec3::Y),
    ]
    .map(|(direction, up)| projection * rh::view::look_to_mat4(position, direction, up))
}
pub(crate) fn zero_uniform(device: &wgpu::Device, size: u64, label: &str) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: &vec![0; size as usize],
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    })
}
pub(crate) fn depth(device: &wgpu::Device, resolution: u32, layers: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bounded point shadow depth array"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            // GLES wgpu-hal otherwise infers cube arrays for square textures
            // whose layer count is a multiple of six, despite a D2Array view.
            // One unused layer keeps comparison sampling and face attachments
            // portable. It is bounded (at most 25 layers), never sampled/drawn.
            depth_or_array_layers: if device.adapter_info().backend == wgpu::Backend::Gl
                && layers.is_multiple_of(6)
            {
                layers + 1
            } else {
                layers
            },
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
pub(crate) fn array_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}
pub(crate) fn comparison_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("point shadow PCF"),
        compare: Some(wgpu::CompareFunction::LessEqual),
        min_filter: wgpu::FilterMode::Linear,
        mag_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}
