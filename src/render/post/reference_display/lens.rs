//! Source-default analytical lens ghosts and separately submitted visibility.
use wgpu::util::DeviceExt;
pub(super) const SHADER: &str = include_str!("lens/common.wgsl");
#[cfg(test)]
mod tests;
pub(super) struct Lens {
    pub uniform: wgpu::Buffer,
    pub visibility: [wgpu::TextureView; 2],
    data: [f32; 16],
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    pub index: usize,
    valid: bool,
    resolved: bool,
    configured: bool,
}
impl Lens {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("reference flare visibility"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("source flare visibility decay"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{SHADER}\n{}", include_str!("lens/visibility.wgsl")).into(),
            ),
        });
        let target = || {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("source visibleSun history"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R32Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        Self {
            uniform: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("source flare inputs"),
                contents: bytemuck::cast_slice(&[0.0_f32; 16]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }),
            visibility: [target(), target()],
            pipeline: super::pipeline(
                device,
                &shader,
                &layout,
                "visibility",
                &[wgpu::TextureFormat::R32Float],
            ),
            layout,
            data: [0.0; 16],
            index: 0,
            valid: false,
            resolved: false,
            configured: false,
        }
    }
    pub fn configure(
        &mut self,
        matrix: glam::Mat4,
        eye: glam::Vec3,
        atmosphere: crate::render::daylight::Atmosphere,
        frame_time: f32,
        eye_in_water: bool,
    ) {
        self.data = frame(matrix, eye, atmosphere, frame_time, eye_in_water);
        self.configured = true;
    }
    pub fn reset(&mut self) {
        self.valid = false;
        self.resolved = false;
    }
    pub fn submitted(&mut self) {
        if self.resolved {
            self.index = 1 - self.index;
            self.valid = true;
            self.resolved = false;
        }
        self.configured = false;
    }
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        depth: Option<&wgpu::TextureView>,
        effects: bool,
    ) {
        let active = effects && self.configured && depth.is_some();
        self.data[11] = f32::from(active);
        self.data[12] = f32::from(self.valid);
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&self.data));
        if !active {
            return;
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("reference visibility input"),
            layout: &self.layout,
            entries: &[
                super::texture_entry(0, depth.unwrap()),
                super::texture_entry(1, &self.visibility[self.index]),
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        super::draw(
            encoder,
            &self.pipeline,
            &group,
            &[&self.visibility[1 - self.index]],
        );
        self.resolved = true;
    }
}
fn frame(
    matrix: glam::Mat4,
    eye: glam::Vec3,
    atmosphere: crate::render::daylight::Atmosphere,
    frame_time: f32,
    eye_in_water: bool,
) -> [f32; 16] {
    // Iris sunPosition is length100 in camera coordinates. GetLightPos divides
    // GL clip XY by clip Z; map WebGPU zero-to-one Z back to negative-one-to-one.
    let clip = matrix * (eye + atmosphere.sun * 100.0).extend(1.0);
    let denominator = 2.0 * clip.z - clip.w;
    let light = if denominator.abs() > 1e-6 {
        clip.truncate().truncate() * (0.5 / denominator)
    } else {
        glam::Vec2::splat(100.0)
    };
    let rows = matrix.transpose();
    let fov = rows.y_axis.truncate().length() / 1.3737387;
    let night = glam::Vec3::new(96.0, 192.0, 255.0) * (0.3 * atmosphere.moon_multiplier() / 255.0);
    [
        light.x,
        light.y,
        -clip.w.signum(),
        fov.max(0.001),
        (atmosphere.sun.y * 10.0 + 0.5).clamp(0.0, 1.0),
        (-atmosphere.sun.y * 10.0 + 0.5).clamp(0.0, 1.0),
        atmosphere.rain_strength.clamp(0.0, 1.0),
        frame_time.max(0.0),
        night.x,
        night.y,
        night.z,
        0.0,
        0.0,
        if eye_in_water {
            atmosphere.fog_exposure.clamp(0.0, 1.0)
        } else {
            1.0
        },
        0.0,
        0.0,
    ]
}
