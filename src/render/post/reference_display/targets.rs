//! Size-dependent reference display attachments; pipelines survive resize.
pub(super) struct Targets {
    pub linear: wgpu::TextureView,
    pub gamma: wgpu::TextureView,
    pub fxaa: wgpu::TextureView,
    pub colors: [wgpu::TextureView; 2],
    pub depths: [wgpu::TextureView; 2],
}
impl Targets {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = |label, format| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: width.max(1),
                        height: height.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        Self {
            linear: texture("reference tone HDR", super::HDR_FORMAT),
            gamma: texture("reference gamma grain", wgpu::TextureFormat::Rgba8Unorm),
            fxaa: texture("reference FXAA", wgpu::TextureFormat::Rgba8Unorm),
            colors: [
                texture("reference history A", wgpu::TextureFormat::Rgba8Unorm),
                texture("reference history B", wgpu::TextureFormat::Rgba8Unorm),
            ],
            depths: [
                texture("reference depth A", wgpu::TextureFormat::R32Float),
                texture("reference depth B", wgpu::TextureFormat::R32Float),
            ],
        }
    }
}
