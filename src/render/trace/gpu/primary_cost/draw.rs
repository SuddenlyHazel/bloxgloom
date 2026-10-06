//! Two shader variants, reused with a runtime packet-bank uniform.
use super::{HEIGHT, POINTS, Row, WIDTH};
use wgpu::util::DeviceExt;

pub(super) struct Inputs<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub frame: &'a wgpu::BindGroup,
    pub materials: &'a wgpu::BindGroup,
    pub dynamic: &'a wgpu::BindGroup,
    pub layouts: [&'a wgpu::BindGroupLayout; 3],
}
pub(super) struct Probe {
    pipeline: wgpu::RenderPipeline,
    packet: wgpu::Buffer,
    group: wgpu::BindGroup,
    targets: [wgpu::Texture; 2],
    views: [wgpu::TextureView; 2],
    readback: [wgpu::Buffer; 2],
}
const ROW_BYTES: u32 = (WIDTH * 16).div_ceil(256) * 256;

impl Probe {
    pub fn new(input: &Inputs<'_>, source: &str, points: [[u32; 4]; POINTS]) -> Self {
        let device = input.device;
        let extra = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test-only primary counter bank"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(272),
                },
                count: None,
            }],
        });
        let words: Vec<u32> = points.into_iter().flatten().chain([0; 4]).collect();
        let packet = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("real raster 2x2 sample coordinates and packet bank"),
            contents: bytemuck::cast_slice(&words),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &extra,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: packet.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("real frame/material/dynamic bindings plus test-only packet"),
            bind_group_layouts: &[
                Some(input.layouts[0]),
                Some(input.layouts[1]),
                Some(input.layouts[2]),
                Some(&extra),
            ],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("actual primary transport private counter probe"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let colors = std::array::from_fn::<_, 2, _>(|_| {
            Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba32Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("64 real primary samples; not a frame timing"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_primary_probe"),
                compilation_options: Default::default(),
                targets: &colors,
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let targets = std::array::from_fn(|_| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("test-only HDR and exact query packet"),
                size: wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        let views = targets
            .each_ref()
            .map(|target| target.create_view(&Default::default()));
        let readback = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("test-only exact float counter readback"),
                size: u64::from(ROW_BYTES * HEIGHT),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        Self {
            pipeline,
            packet,
            group,
            targets,
            views,
            readback,
        }
    }

    pub fn run(&self, input: &Inputs<'_>, mode: u32) -> Vec<Row> {
        input
            .queue
            .write_buffer(&self.packet, 256, bytemuck::cast_slice(&[mode, 0, 0, 0]));
        let mut encoder = input.device.create_command_encoder(&Default::default());
        {
            let attachments = self.views.each_ref().map(|view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sparse actual primary 2x2 quads"),
                color_attachments: &attachments,
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, input.frame, &[]);
            pass.set_bind_group(1, input.materials, &[]);
            pass.set_bind_group(2, input.dynamic, &[]);
            pass.set_bind_group(3, &self.group, &[]);
            pass.draw(0..3, 0..1);
        }
        for (texture, buffer) in self.targets.iter().zip(&self.readback) {
            encoder.copy_texture_to_buffer(
                texture.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(ROW_BYTES),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                texture.size(),
            );
        }
        let submission = input.queue.submit([encoder.finish()]);
        for buffer in &self.readback {
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, |result| result.unwrap());
        }
        input
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .expect("64 actual primary counter samples must complete");
        let output: [Vec<[f32; 4]>; 2] = self.readback.each_ref().map(|buffer| {
            let mapped = buffer.slice(..).get_mapped_range().unwrap();
            let rows = mapped
                .chunks_exact(ROW_BYTES as usize)
                .flat_map(|row| {
                    bytemuck::cast_slice::<u8, [f32; 4]>(&row[..WIDTH as usize * 16]).to_vec()
                })
                .collect();
            drop(mapped);
            buffer.unmap();
            rows
        });
        output[0]
            .iter()
            .zip(&output[1])
            .map(|(first, second)| [*first, *second])
            .collect()
    }
}
