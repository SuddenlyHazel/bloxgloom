//! Bounded, camera-local directional shadows. Voxel light remains authoritative
//! for portals/caves and indirect light; this map only occludes the sun term.
use glam::{Mat4, Vec3, camera::rh};
use wgpu::util::DeviceExt;

use super::{Camera, DEPTH_FORMAT, daylight::Atmosphere};
use crate::config::SunShadowQuality;

mod draw;
mod softness;
use softness::Softness;
#[cfg(test)]
mod tests;

pub(super) const SHADER: &str = include_str!("sun_shadow/shader.wgsl");
const SOFTNESS_OFFSET: u64 = 96 + super::scene_contact::UNIFORM_BYTES;
pub(super) const UNIFORM_BYTES: u64 = SOFTNESS_OFFSET + 16;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    pub resolution: u32,
    pub distance: f32,
    pub filter: f32,
    softness: Softness,
}

impl Settings {
    pub(crate) fn for_quality(quality: SunShadowQuality, maximum: u32) -> Self {
        let (requested, distance, filter) = match quality {
            SunShadowQuality::Off => (1, 0.0, 0.0),
            SunShadowQuality::Low => (1024, 24.0, 0.0),
            SunShadowQuality::Medium => (2048, 40.0, 1.0),
            SunShadowQuality::High => (4096, 56.0, 1.5),
        };
        // Depth32Float sampling/rendering is a baseline WGPU capability. A
        // smaller device limit falls back explicitly instead of failing startup.
        let resolution = requested.min(maximum).max(1);
        Self {
            resolution,
            distance: if resolution < 512 { 0.0 } else { distance },
            filter,
            softness: Softness::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Projection {
    pub matrix: Mat4,
    pub eye: Vec3,
    pub settings: Settings,
    pub enabled: bool,
    horizon_weight: f32,
}

impl Projection {
    pub(crate) fn new(camera: Camera, atmosphere: Atmosphere, settings: Settings) -> Self {
        // Fade grazing-angle shadows before skipping their expensive map. The
        // atmosphere still has twilight energy here, so a boolean cutoff pops.
        let horizon = ((atmosphere.sun.y - 0.01) / 0.09).clamp(0.0, 1.0);
        let horizon_weight = horizon * horizon * (3.0 - 2.0 * horizon);
        let enabled = settings.distance > 0.0 && horizon_weight > 0.0 && atmosphere.strength > 0.0;
        let radius = settings.distance.max(1.0) * 1.25;
        let sun = atmosphere.sun.normalize_or_zero();
        let up = if sun.dot(Vec3::Y).abs() > 0.98 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let rotation = rh::view::look_to_mat4(Vec3::ZERO, -sun, up);
        let center = rotation.transform_point3(camera.position);
        let texel = 2.0 * radius / settings.resolution as f32;
        let snapped = Vec3::new(
            (center.x / texel).round() * texel,
            (center.y / texel).round() * texel,
            (center.z / 4.0).round() * 4.0,
        );
        let view = Mat4::from_translation(-snapped) * rotation;
        let matrix = rh::proj::directx::orthographic(
            -radius,
            radius,
            -radius,
            radius,
            -radius * 3.0,
            radius * 3.0,
        ) * view;
        Self {
            matrix,
            eye: camera.position,
            settings,
            enabled,
            horizon_weight,
        }
    }

    fn data(self) -> [f32; 24] {
        let mut data = [0.0; 24];
        data[..16].copy_from_slice(&self.matrix.to_cols_array());
        data[16..20].copy_from_slice(&[
            1.0 / self.settings.resolution as f32,
            self.settings.distance,
            if self.enabled {
                self.horizon_weight
            } else {
                0.0
            },
            self.settings.filter,
        ]);
        data[20..23].copy_from_slice(&self.eye.to_array());
        data[23] = self.settings.distance * 7.5;
        data
    }

    pub(crate) fn contains_chunk(self, key: crate::world::ChunkKey, padding: f32) -> bool {
        self.enabled && super::visibility::chunk_visible_padded(self.matrix, key, padding)
    }
}

pub(crate) struct SunShadows {
    pub view: wgpu::TextureView,
    pub camera_group: wgpu::BindGroup,
    pub caster_group: wgpu::BindGroup,
    pub(super) uniform: wgpu::Buffer,
    pub projection: Projection,
    quality: SunShadowQuality,
    contact_strength: f32,
}

impl SunShadows {
    pub(crate) fn new(
        device: &wgpu::Device,
        camera: &wgpu::Buffer,
        quality: SunShadowQuality,
    ) -> Self {
        let mut settings = Settings::for_quality(quality, device.limits().max_texture_dimension_2d);
        settings.softness = Softness::configured();
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nearby sun projection"),
            contents: &[0; UNIFORM_BYTES as usize],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let view = depth(device, settings.resolution);
        let dummy = depth(device, 1);
        let sampler = sampler(device);
        let camera_group = group(device, camera, &uniform, &view, &sampler);
        // The caster pass never samples its attachment. Use a distinct dummy
        // texture even though the depth entry points do not access binding 2.
        let caster_group = group(device, camera, &uniform, &dummy, &sampler);
        Self {
            view,
            camera_group,
            caster_group,
            uniform,
            quality,
            contact_strength: super::scene_contact::configured_strength(),
            projection: Projection::new(
                Camera {
                    position: Vec3::ZERO,
                    yaw: 0.0,
                    pitch: 0.0,
                    fov_y_radians: 1.0,
                },
                Atmosphere::at(crate::daylight::INITIAL_MS),
                settings,
            ),
        }
    }

    pub(crate) fn bind_local(
        &mut self,
        device: &wgpu::Device,
        camera: &wgpu::Buffer,
        local: &super::local_shadow::LocalShadows,
    ) {
        self.camera_group = group_with_local(
            device,
            camera,
            &self.uniform,
            &self.view,
            &sampler(device),
            &local.uniform,
            &local.view,
            &local.sampler,
        );
    }

    pub(crate) fn configure(
        &mut self,
        device: &wgpu::Device,
        camera: &wgpu::Buffer,
        quality: SunShadowQuality,
    ) -> bool {
        if self.quality == quality {
            return false;
        }
        *self = Self::new(device, camera, quality);
        true
    }

    pub(crate) fn update(&mut self, queue: &wgpu::Queue, camera: Camera, atmosphere: Atmosphere) {
        self.projection = Projection::new(camera, atmosphere, self.projection.settings);
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&self.projection.data()),
        );
        queue.write_buffer(
            &self.uniform,
            SOFTNESS_OFFSET,
            bytemuck::cast_slice(&self.projection.settings.softness.data()),
        );
    }

    pub(crate) fn set_contacts(
        &self,
        queue: &wgpu::Queue,
        patches: &[super::contact_shadow::Patch],
    ) {
        let data = super::scene_contact::data(patches, self.contact_strength);
        queue.write_buffer(&self.uniform, 96, bytemuck::cast_slice(&data));
    }

    pub(crate) fn begin<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
    ) -> Option<wgpu::RenderPass<'a>> {
        self.begin_timed(encoder, None)
    }

    pub(crate) fn begin_timed<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'a>>,
    ) -> Option<wgpu::RenderPass<'a>> {
        self.projection.enabled.then(|| {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shared nearby sun depth"),
                color_attachments: &[],
                timestamp_writes: timestamps,
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
        })
    }
}

pub(crate) fn camera_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let buffer = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera and shared sun shadows"),
        entries: &[
            buffer(0),
            buffer(1),
            buffer(4),
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    })
}

pub(crate) fn fallback_camera_group(
    device: &wgpu::Device,
    camera: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("disabled sun shadow uniform"),
        contents: &[0; UNIFORM_BYTES as usize],
        usage: wgpu::BufferUsages::UNIFORM,
    });
    group(
        device,
        camera,
        &uniform,
        &depth(device, 1),
        &sampler(device),
    )
}

pub(super) fn group(
    device: &wgpu::Device,
    camera: &wgpu::Buffer,
    uniform: &wgpu::Buffer,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let local_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("disabled local shadows"),
        contents: &[0; super::local_shadow::UNIFORM_BYTES as usize],
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let local_depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("disabled local depth"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
    group_with_local(
        device,
        camera,
        uniform,
        view,
        sampler,
        &local_uniform,
        &local_depth,
        sampler,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn group_with_local(
    device: &wgpu::Device,
    camera: &wgpu::Buffer,
    uniform: &wgpu::Buffer,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    local_uniform: &wgpu::Buffer,
    local_view: &wgpu::TextureView,
    local_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera sun and local bindings"),
        layout: &camera_layout(device),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 4,
                resource: local_uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(local_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(local_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
fn depth(device: &wgpu::Device, resolution: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("bounded sun depth map"),
            size: wgpu::Extent3d {
                width: resolution,
                height: resolution,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}
fn sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sun depth PCF"),
        compare: Some(wgpu::CompareFunction::LessEqual),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    })
}

pub(super) fn depth_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: Default::default(),
        bias: wgpu::DepthBiasState {
            constant: 1,
            // Receiver-plane correction covers the PCF tap offsets. One texel
            // of caster slope bias covers the larger depth axis; the receiver
            // covers the smaller axis in a bilinear texel footprint.
            slope_scale: 1.0,
            clamp: 0.0,
        },
    }
}

#[cfg(test)]
pub(in crate::render) mod gpu_tests;
