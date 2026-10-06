//! Production terrain caster/receiver readback, including cutout and edited geometry.
use super::*;
use crate::render::{VERTEX_FLOATS, pipeline};

const SIZE: u32 = 128;
mod banding;
mod reference;
mod softness;

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    camera: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    caster_pipelines: (wgpu::RenderPipeline, wgpu::RenderPipeline),
    textures: wgpu::BindGroup,
    material: Option<crate::render::custom::Gpu>,
    shadows: SunShadows,
    floor: wgpu::Buffer,
    indices: wgpu::Buffer,
    color: wgpu::Texture,
    depth: wgpu::TextureView,
    readback: wgpu::Buffer,
    cast_floor: bool,
    studio_light: Option<(f32, f32)>,
}

impl Fixture {
    fn new() -> Self {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            apply_limit_buckets: false,
            ..Default::default()
        }))
        .unwrap();
        eprintln!("sun shadow GPU: {:?}", adapter.get_info());
        let required_limits = crate::render::material_device_limits(
            adapter.limits(),
            crate::render::material_texture_layers(crate::content::catalog()) as usize,
        )
        .unwrap();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits,
            ..Default::default()
        }))
        .unwrap();
        let (pipeline, _, camera, _, textures) =
            pipeline::create_voxel_pipeline(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let caster_pipelines = pipeline::create_sun_shadow_pipelines(&device, &pipeline, None);
        let shadows = SunShadows::new(&device, &camera, SunShadowQuality::Low);
        let floor = Self::quad(&device, 4.0, 0.0, 0.0, 2.0, [0.25; 2], 1.0, 0.0);
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0u32, 1, 2, 0, 2, 3]),
            usage: wgpu::BufferUsages::INDEX,
        });
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = crate::render::visibility::create_depth(&device, SIZE, SIZE);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(SIZE * SIZE * 4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            device,
            queue,
            camera,
            pipeline,
            caster_pipelines,
            textures,
            material: None,
            shadows,
            floor,
            indices,
            color,
            depth,
            readback,
            cast_floor: false,
            studio_light: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn quad(
        device: &wgpu::Device,
        radius: f32,
        height: f32,
        x: f32,
        layer: f32,
        uv: [f32; 2],
        sky: f32,
        glow: f32,
    ) -> wgpu::Buffer {
        let mut vertices = Vec::with_capacity(4 * VERTEX_FLOATS);
        for (px, pz) in [
            (-radius, -radius),
            (-radius, radius),
            (radius, radius),
            (radius, -radius),
        ] {
            vertices.extend_from_slice(&[
                x + px,
                height,
                pz,
                0.0,
                1.0,
                0.0,
                uv[0],
                uv[1],
                layer,
                sky,
                glow,
                0.0,
                0.0,
                {
                    let rgb = [1.0, 0.57, 0.23].map(|v| (v * glow * 255.0).round() as u32);
                    (rgb[0] | (rgb[1] << 8) | (rgb[2] << 16)) as f32
                },
                0.0,
            ]);
        }
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }

    fn render(
        &mut self,
        caster: Option<(&wgpu::Buffer, bool)>,
        time: u64,
        quality: SunShadowQuality,
    ) -> Vec<u8> {
        self.shadows.configure(&self.device, &self.camera, quality);
        let camera = Camera {
            position: Vec3::new(0.0, 5.0, 0.0),
            yaw: 0.0,
            pitch: -1.5,
            fov_y_radians: 1.0,
        };
        let atmosphere = Atmosphere::at(time);
        let view = rh::view::look_at_mat4(camera.position, Vec3::ZERO, Vec3::Z);
        let matrix = rh::proj::directx::orthographic(-4.0, 4.0, -4.0, 4.0, 0.1, 20.0) * view;
        let mut camera_data = atmosphere.camera_data(matrix, camera.position);
        if let Some((diffuse, directional)) = self.studio_light {
            crate::render::tests::studio::light(&mut camera_data, diffuse, directional);
        }
        self.queue
            .write_buffer(&self.camera, 0, bytemuck::cast_slice(&camera_data));
        self.shadows.update(&self.queue, camera, atmosphere);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if let Some(mut pass) = self.shadows.begin(&mut encoder)
            && let Some((vertices, cutout)) = caster
        {
            pass.set_pipeline(if cutout {
                &self.caster_pipelines.1
            } else {
                &self.caster_pipelines.0
            });
            pass.set_bind_group(0, &self.shadows.caster_group, &[]);
            pass.set_bind_group(1, &self.textures, &[]);
            if let Some(gpu) = &self.material {
                pass.set_bind_group(2, &gpu.group, &[]);
            }
            if self.cast_floor {
                pass.set_pipeline(&self.caster_pipelines.0);
                pass.set_vertex_buffer(0, self.floor.slice(..));
                pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..6, 0, 0..1);
                pass.set_pipeline(if cutout {
                    &self.caster_pipelines.1
                } else {
                    &self.caster_pipelines.0
                });
            }
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        let color = self.color.create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.shadows.camera_group, &[]);
            pass.set_bind_group(1, &self.textures, &[]);
            if let Some(gpu) = &self.material {
                pass.set_bind_group(2, &gpu.group, &[]);
            }
            pass.set_vertex_buffer(0, self.floor.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
            },
            self.color.size(),
        );
        self.queue.submit([encoder.finish()]);
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let data = self.readback.slice(..).get_mapped_range().unwrap().to_vec();
        self.readback.unmap();
        data
    }
}

#[test]
fn gpu_terrain_shadow_edits_cutouts_and_sky_glow_invariance() {
    let mut scene = Fixture::new();
    let opaque = Fixture::quad(&scene.device, 0.6, 2.0, 0.0, 2.0, [0.25; 2], 1.0, 0.0);
    let shifted = Fixture::quad(&scene.device, 0.6, 2.0, -2.0, 2.0, [0.25; 2], 1.0, 0.0);
    let time = crate::daylight::INITIAL_MS;
    let clear = scene.render(None, time, SunShadowQuality::Low);
    let shaded = scene.render(Some((&opaque, false)), time, SunShadowQuality::Low);
    assert!(
        clear
            .chunks_exact(4)
            .zip(shaded.chunks_exact(4))
            .filter(|(a, b)| a[0].saturating_sub(b[0]) > 4)
            .count()
            > 150,
        "terrain must receive the production occluder"
    );
    assert_eq!(
        clear,
        scene.render(Some((&opaque, false)), time, SunShadowQuality::Off)
    );
    assert_ne!(
        shaded,
        scene.render(Some((&shifted, false)), time, SunShadowQuality::Low)
    );
    assert_eq!(
        clear,
        scene.render(None, time, SunShadowQuality::Low),
        "removed occluders must not retain old depth"
    );
    let pixels = crate::render::material::material_tiles_for(crate::content::catalog());
    let tile_size = crate::render::material::TEXTURE_SIZE as usize;
    let tile_bytes = tile_size * tile_size * 4;
    let leaves = &pixels[12 * tile_bytes..13 * tile_bytes];
    for transparent in [true, false] {
        let texel = leaves
            .chunks_exact(4)
            .position(|p| if transparent { p[3] == 0 } else { p[3] >= 128 })
            .unwrap();
        let uv = [
            ((texel % tile_size) as f32 + 0.5) / tile_size as f32,
            ((texel / tile_size) as f32 + 0.5) / tile_size as f32,
        ];
        let cutout = Fixture::quad(&scene.device, 0.6, 2.0, 0.0, 12.0, uv, 1.0, 0.0);
        let image = scene.render(Some((&cutout, true)), time, SunShadowQuality::Low);
        if transparent {
            assert_eq!(
                clear, image,
                "transparent foliage must not cast a solid card"
            );
        } else {
            // Both casters must use the leaf material's wind transform. The
            // stationary stone quad is no longer the same geometry.
            let solid_card = scene.render(Some((&cutout, false)), time, SunShadowQuality::Low);
            assert_eq!(
                solid_card, image,
                "opaque leaf texels must cast their actual moving card"
            );
        }
    }
    for (sky, glow) in [(0.0, 0.0), (0.0, 1.0)] {
        scene.floor = Fixture::quad(&scene.device, 4.0, 0.0, 0.0, 2.0, [0.25; 2], sky, glow);
        let unshadowed = scene.render(None, time, SunShadowQuality::Low);
        assert_eq!(
            unshadowed,
            scene.render(Some((&opaque, false)), time, SunShadowQuality::Low),
            "sun shadow changed cave/torch energy"
        );
    }
    scene.floor = Fixture::quad(&scene.device, 4.0, 0.0, 0.0, 2.0, [0.25; 2], 1.0, 0.0);
    let night = crate::daylight::CYCLE_MS * 3 / 4;
    assert_eq!(
        scene.render(None, night, SunShadowQuality::Low),
        scene.render(Some((&opaque, false)), night, SunShadowQuality::High)
    );
}

/// Exercise a custom cutout through both production passes without exposing
/// graphics harness details outside test builds.
pub(in crate::render) fn verify_custom_alpha(prepared: &crate::render::custom::Prepared) {
    let mut scene = Fixture::new();
    // Unshadowed light must exceed the hook's .7 alpha threshold while fully
    // shadowed light stays below it. Passing shadowed light into the alpha hook
    // would then remove covered receiver pixels, rather than merely dim them.
    scene.studio_light = Some((0.50, 2.4));
    let shadowed_hook_light = 0.012 + 0.50;
    let noon = Atmosphere::at(crate::daylight::INITIAL_MS);
    let unshadowed_hook_light =
        shadowed_hook_light + 2.4 * noon.light_direction().y.max(0.0) / std::f32::consts::PI;
    assert!(
        shadowed_hook_light < 0.7 && unshadowed_hook_light > 0.7,
        "custom-alpha fixture must straddle its light threshold: {shadowed_hook_light}..{unshadowed_hook_light}"
    );
    let ((opaque, cutout, _, _, _), mut gpu) = pipeline::create_custom_voxel_pipeline(
        &scene.device,
        &scene.queue,
        wgpu::TextureFormat::Rgba8Unorm,
        crate::content::catalog(),
        prepared,
    )
    .unwrap();
    scene.caster_pipelines =
        pipeline::create_sun_shadow_pipelines(&scene.device, &opaque, Some(prepared));
    scene.pipeline = cutout;
    gpu.update(&scene.queue);
    scene.material = Some(gpu);
    let occluder = Fixture::quad(&scene.device, 0.6, 2.0, 0.0, 2.0, [0.25; 2], 1.0, 0.0);
    let time = crate::daylight::INITIAL_MS;
    let clear = scene.render(Some((&occluder, true)), time, SunShadowQuality::Off);
    let shadow = scene.render(Some((&occluder, true)), time, SunShadowQuality::Low);
    assert!(
        shadow.chunks_exact(4).all(|pixel| pixel[0] > 30),
        "custom cutout coverage changed because shadowed light reached its alpha hook"
    );
    assert!(
        clear
            .chunks_exact(4)
            .zip(shadow.chunks_exact(4))
            .filter(|(a, b)| a[0].saturating_sub(b[0]) > 4)
            .count()
            > 150,
        "custom cutout should cast and receive sun shadows"
    );
}
