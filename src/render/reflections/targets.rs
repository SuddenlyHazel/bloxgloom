//! Size-dependent reflection data and bounded HDR radiance pyramid.
use super::super::post::HDR_FORMAT;

pub(super) struct Targets {
    pub normal: wgpu::TextureView,
    pub response: wgpu::TextureView,
    pub delta: wgpu::TextureView,
    pub pyramid_view: wgpu::TextureView,
    pub levels: Vec<wgpu::TextureView>,
}
impl Targets {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = |label, width: u32, height: u32, mip_level_count| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let normal = texture("reflection mapped normal and roughness", width, height, 1)
            .create_view(&Default::default());
        let response = texture(
            "reflection integrated BRDF and sky visibility",
            width,
            height,
            1,
        )
        .create_view(&Default::default());
        let delta = texture(
            "half-resolution scene reflection replacement",
            width.div_ceil(2),
            height.div_ceil(2),
            1,
        )
        .create_view(&Default::default());
        let count = (32 - width.max(height).max(1).leading_zeros()).min(7);
        let pyramid = texture(
            "linear HDR reflection radiance pyramid",
            width,
            height,
            count,
        );
        let levels = (0..count)
            .map(|base_mip_level| {
                pyramid.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("reflection radiance level"),
                    base_mip_level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let pyramid_view = pyramid.create_view(&Default::default());
        Self {
            normal,
            response,
            delta,
            pyramid_view,
            levels,
        }
    }
}
