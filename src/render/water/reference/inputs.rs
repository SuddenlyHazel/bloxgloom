//! Shared local runtime noise and source frame controls for near/LOD water.
use wgpu::util::DeviceExt;
#[derive(Clone)]
pub(crate) struct Inputs {
    pub(crate) noise: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    pub(crate) frame: wgpu::Buffer,
    available: bool,
    pub(crate) opaque_depth: wgpu::TextureView,
    pub(crate) reflection: wgpu::TextureView,
    pub(crate) reflection_sampler: wgpu::Sampler,
}
impl Inputs {
    pub(super) fn from_noise(
        device: &wgpu::Device,
        noise: &crate::render::sky::ReferenceNoise,
    ) -> Self {
        let fallback = Self::fallback(device);
        Self {
            noise: noise.view.clone(),
            sampler: noise.sampler.clone(),
            frame: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shared source water frame"),
                contents: bytemuck::cast_slice(&[0.0f32; 80]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            available: noise.available,
            opaque_depth: fallback.opaque_depth,
            reflection: fallback.reflection,
            reflection_sampler: fallback.reflection_sampler,
        }
    }
    pub(crate) fn fallback(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("unconfigured reference water noise"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self {
            noise: texture.create_view(&Default::default()),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            frame: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("unconfigured reference water frame"),
                contents: bytemuck::cast_slice(&[0.0f32; 80]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            available: false,
            opaque_depth: device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("unconfigured source water opaque depth"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: crate::render::DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default()),
            reflection: device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("unconfigured source water opaque reflection"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: crate::render::post::HDR_FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default()),
            reflection_sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
        }
    }
    pub(crate) fn prepare(&self, queue: &wgpu::Queue, frame: crate::render::water::Frame) {
        let sky = crate::render::sky::sky_camera_data_at_sample(
            frame.camera,
            frame.size[0],
            frame.size[1],
            frame.atmosphere,
            frame.sample,
        );
        let mut data = [0.0f32; 80];
        data[..40].copy_from_slice(&sky);
        data[40] = f32::from(self.available);
        data[41] = f32::from(frame.eye_in_water);
        data[44..60].copy_from_slice(&frame.view_projection.to_cols_array());
        data[60..76].copy_from_slice(&frame.view_projection.inverse().to_cols_array());
        data[76..79].copy_from_slice(&frame.camera.position.to_array());
        queue.write_buffer(&self.frame, 0, bytemuck::cast_slice(&data));
    }
    pub(crate) fn layout_entries(start: u32) -> [wgpu::BindGroupLayoutEntry; 6] {
        [
            wgpu::BindGroupLayoutEntry {
                binding: start,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: start + 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: start + 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: start + 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: start + 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: start + 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ]
    }
    pub(crate) fn entries(&self, start: u32) -> [wgpu::BindGroupEntry<'_>; 6] {
        [
            wgpu::BindGroupEntry {
                binding: start,
                resource: wgpu::BindingResource::TextureView(&self.noise),
            },
            wgpu::BindGroupEntry {
                binding: start + 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: start + 2,
                resource: self.frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: start + 3,
                resource: wgpu::BindingResource::TextureView(&self.opaque_depth),
            },
            wgpu::BindGroupEntry {
                binding: start + 4,
                resource: wgpu::BindingResource::TextureView(&self.reflection),
            },
            wgpu::BindGroupEntry {
                binding: start + 5,
                resource: wgpu::BindingResource::Sampler(&self.reflection_sampler),
            },
        ]
    }
}
