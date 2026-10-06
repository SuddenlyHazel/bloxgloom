//! Bounded read-only static LOD pages, separate from near-space certificates.
use crate::render::trace::scene::{Scene, pages};
use wgpu::util::DeviceExt;
pub(super) mod vector;

pub(super) const SHADER: &str = include_str!("../lod.wgsl");

pub(super) struct LodGeometry {
    pages: [wgpu::Buffer; pages::MAX_PAGES],
}
impl LodGeometry {
    pub(super) fn new(device: &wgpu::Device, scenes: &[Scene]) -> Self {
        assert!(scenes.len() <= pages::MAX_PAGES);
        Self {
            pages: std::array::from_fn(|page| {
                let words = scenes
                    .get(page)
                    .map_or_else(|| vec![0; 4], pages::packed_words);
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("static distant ray page"),
                    contents: bytemuck::cast_slice(&words),
                    usage: wgpu::BufferUsages::STORAGE,
                })
            }),
        }
    }
    pub(super) fn entries(&self) -> [wgpu::BindGroupEntry<'_>; pages::MAX_PAGES] {
        std::array::from_fn(|page| wgpu::BindGroupEntry {
            binding: 11 + page as u32,
            resource: self.pages[page].as_entire_binding(),
        })
    }
    pub(super) fn layout_entries() -> [wgpu::BindGroupLayoutEntry; pages::MAX_PAGES] {
        std::array::from_fn(|page| wgpu::BindGroupLayoutEntry {
            binding: 11 + page as u32,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(16),
            },
            count: None,
        })
    }
}

#[cfg(test)]
mod tests;
