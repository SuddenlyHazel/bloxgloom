//! Opt-in real-network edit-to-GPU QA. CPU meshes use the production builder
//! on a scoped worker directly; this fixture does not exercise ClientState's
//! asynchronous scheduler or a presented window. GPU completion precedes image
//! readback, PNG encoding, and framebuffer assertions in the latency endpoint.
use super::{FaceColors, Gpu};
use crate::{
    lod::LodTile,
    render::{Camera, daylight::Atmosphere, post},
};
use glam::Vec3;
use std::{path::PathBuf, time::Instant};
const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(crate) struct EditGpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    gpu: Gpu,
    post: post::PostProcess,
    sky: super::super::SkyRenderer,
    color: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    colors: FaceColors,
    directory: PathBuf,
    before: Vec<u8>,
}
impl EditGpu {
    pub(crate) fn from_env() -> Option<Self> {
        let directory = std::env::var_os("BLOXGLOOM_LOD_GPU_QA").filter(|p| !p.is_empty())?;
        Some(pollster::block_on(Self::new(PathBuf::from(directory))))
    }
    async fn new(directory: PathBuf) -> Self {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                apply_limit_buckets: false,
                ..Default::default()
            })
            .await
            .expect("opt-in LOD GPU QA adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: super::super::material_device_limits(
                    adapter.limits(),
                    super::super::material_texture_layers(crate::content::catalog()) as usize,
                )
                .expect("opt-in LOD GPU QA material limits"),
                ..Default::default()
            })
            .await
            .expect("opt-in LOD GPU QA device");
        let (materials, _, _, _, textures) =
            super::super::create_voxel_pipeline(&device, &queue, post::HDR_FORMAT);
        let gpu = Gpu::new(&device, post::HDR_FORMAT, &materials, &textures);
        let post = post::PostProcess::new(&device, WIDTH, HEIGHT, FORMAT);
        let sky = super::super::SkyRenderer::new(&device, WIDTH, HEIGHT, post::HDR_FORMAT);
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("network edit GPU QA framebuffer"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = color.create_view(&Default::default());
        let depth = super::super::visibility::create_depth(&device, WIDTH, HEIGHT);
        let colors = std::thread::scope(|scope| {
            scope
                .spawn(|| FaceColors::new(crate::content::catalog()))
                .join()
                .unwrap()
        });
        Self {
            device,
            queue,
            gpu,
            post,
            sky,
            color,
            view,
            depth,
            colors,
            directory,
            before: vec![],
        }
    }
    /// All preparation and baseline image work happen before the edit timer.
    pub(crate) fn warm(&mut self, tile: &LodTile) {
        self.draw(tile);
        self.before = self.capture("before");
    }
    pub(crate) fn updated(&mut self, tile: &LodTile, started: Instant) {
        let completed = self.draw(tile);
        let elapsed = completed.duration_since(started);
        let after = self.capture("after");
        let changed = self
            .before
            .chunks_exact(4)
            .zip(after.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 32,
            "accepted edit did not change the production LOD framebuffer ({changed} pixels)"
        );
        eprintln!(
            "LOD accepted edit -> first client GPU-drawn: {:.3}ms; changed={changed} pixels; production CPU meshing on scoped worker; offscreen GPU completion, excludes image readback/PNG; images={}",
            elapsed.as_secs_f64() * 1000.0,
            self.directory.display()
        );
    }
    fn draw(&mut self, tile: &LodTile) -> Instant {
        let mesh = std::thread::scope(|scope| {
            scope
                .spawn(|| super::mesh(tile, &[], crate::content::catalog(), &self.colors))
                .join()
                .unwrap()
        })
        .expect("network tile mesh admission");
        self.gpu.enqueue(mesh).expect("network GPU tile admission");
        assert_eq!(self.gpu.upload(&self.device), 1);
        let position = Vec3::new(8.0, 98.0, -8.0);
        let direction = (Vec3::new(3.0, 95.5, 1.0) - position).normalize();
        let camera = Camera {
            position,
            yaw: direction.z.atan2(direction.x),
            pitch: direction.y.asin(),
            fov_y_radians: 50.0f32.to_radians(),
        };
        self.gpu.prepare(
            &self.queue,
            camera,
            WIDTH,
            HEIGHT,
            Atmosphere::at(crate::daylight::INITIAL_MS),
            std::iter::empty(),
        );
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("network LOD edit draw"),
            });
        self.queue.write_buffer(
            &self.sky.camera,
            0,
            bytemuck::cast_slice(&super::super::sky_camera_data(
                camera,
                WIDTH,
                HEIGHT,
                Atmosphere::at(crate::daylight::INITIAL_MS),
            )),
        );
        self.sky.prepare(&self.device, &mut encoder, WIDTH, HEIGHT);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("network production LOD HDR pass"),
                color_attachments: &super::super::scene_ao::attachments(
                    &self.post.scene,
                    &self.post.ambient.indirect,
                    &self.post.reflections.normal,
                    &self.post.reflections.response,
                    super::super::SKY_COLOR,
                ),
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.sky.pipeline);
            pass.set_bind_group(0, &self.sky.group, &[]);
            pass.draw(0..3, 0..1);
            assert!(self.gpu.draw(&mut pass) > 0);
        }
        self.post
            .reflections
            .configure(camera.position, Atmosphere::at(crate::daylight::INITIAL_MS));
        self.post.configure_reference_ao(
            Atmosphere::at(crate::daylight::INITIAL_MS).camera_data(
                super::super::view_projection(camera, WIDTH, HEIGHT),
                camera.position,
            ),
            camera.fov_y_radians,
        );
        self.post.resolve_ambient(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.depth,
            super::super::view_projection(camera, WIDTH, HEIGHT),
        );
        self.gpu.draw_water_pass(
            &mut encoder,
            &self.post.scene,
            &self.post.ambient.indirect,
            &self.post.reflections.normal,
            &self.post.reflections.response,
            &self.depth,
        );
        self.post.resolve_reflections(
            &self.device,
            &self.queue,
            &mut encoder,
            &self.depth,
            super::super::view_projection(camera, WIDTH, HEIGHT),
        );
        self.post
            .encode(&self.device, &self.queue, &mut encoder, &self.view);
        let submission = self.queue.submit([encoder.finish()]);
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .expect("network LOD GPU completion");
        Instant::now()
    }
    fn capture(&self, name: &str) -> Vec<u8> {
        crate::preview::capture::save_and_read(
            &self.device,
            &self.queue,
            &self.color,
            WIDTH,
            HEIGHT,
            &self.directory.join(format!("edit-{name}.png")),
        )
        .expect("LOD edit GPU QA image")
    }
}
