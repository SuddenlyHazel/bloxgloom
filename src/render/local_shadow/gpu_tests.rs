//! Real production depth and receiver passes, read back on all six point-light faces.
use super::*;
use crate::config::SunShadowQuality;
use crate::render::{daylight::Atmosphere, pipeline, sun_shadow::SunShadows};

const SIZE: u32 = 128;

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    camera: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    caster_pipelines: (wgpu::RenderPipeline, wgpu::RenderPipeline),
    textures: wgpu::BindGroup,
    shadows: SunShadows,
    floor: wgpu::Buffer,
    indices: wgpu::Buffer,
    color: wgpu::Texture,
    depth: wgpu::TextureView,
    readback: wgpu::Buffer,
    local: LocalShadows,
    direction: Vec3,
    actors: Option<crate::render::avatars::AvatarRenderer>,
    source_color: [f32; 3],
}

impl Fixture {
    fn new() -> Self {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        Self::from_adapter(adapter)
    }

    fn from_adapter(adapter: wgpu::Adapter) -> Self {
        eprintln!("point shadow GPU: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let (pipeline, _, camera, _, textures) =
            pipeline::create_voxel_pipeline(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let caster_pipelines = pipeline::create_sun_shadow_pipelines(&device, &pipeline, None);
        let mut shadows = SunShadows::new(&device, &camera, SunShadowQuality::Off);
        let local = LocalShadows::new_with_settings(
            &device,
            &camera,
            Settings {
                count: 1,
                resolution: 128,
                range: 12.0,
                updates: 1,
            },
        );
        shadows.bind_local(&device, &camera, &local);
        let floor = Self::quad(&device, -Vec3::Y, 4.0, 2.5, 0.0, 2.0, [0.25; 2], 1.0, 1.0);
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
            shadows,
            floor,
            indices,
            color,
            depth,
            readback,
            local,
            direction: -Vec3::Y,
            actors: None,
            source_color: [1.0, 0.63, 0.31],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn quad(
        device: &wgpu::Device,
        direction: Vec3,
        distance: f32,
        radius: f32,
        offset: f32,
        layer: f32,
        uv: [f32; 2],
        sky: f32,
        glow: f32,
    ) -> wgpu::Buffer {
        let normal = -direction;
        let up = if direction.y.abs() > 0.5 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let right = up.cross(normal);
        let mut vertices = Vec::new();
        let bytes = normal.to_array().map(|v| (v * 127.0) as i8 as u8);
        let packed_direction = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
        for (x, y) in [
            (-radius, -radius),
            (radius, -radius),
            (radius, radius),
            (-radius, radius),
        ] {
            let position = direction * distance + right * (x + offset) + up * y;
            vertices.extend_from_slice(&[
                position.x,
                position.y,
                position.z,
                normal.x,
                normal.y,
                normal.z,
                uv[0],
                uv[1],
                layer,
                sky,
                glow,
                0x182028 as f32,
                0x182028 as f32,
                0x50a0ff as f32,
                packed_direction as f32,
            ]);
        }
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }

    fn render(&mut self, caster: Option<(&wgpu::Buffer, bool)>, enabled: bool) -> Vec<u8> {
        let atmosphere = Atmosphere::at(crate::daylight::INITIAL_MS);
        let up = if self.direction.y.abs() > 0.5 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let matrix = rh::proj::directx::orthographic(-2.5, 2.5, -2.5, 2.5, 0.1, 20.0)
            * rh::view::look_at_mat4(Vec3::ZERO, self.direction, up);
        self.queue.write_buffer(
            &self.camera,
            0,
            bytemuck::cast_slice(&atmosphere.camera_data(matrix, Vec3::ZERO)),
        );
        let sources = [Source {
            position: Vec3::ZERO,
            range: 12.0,
            color: self.source_color,
        }];
        // The first refresh initializes the map, the next gives it full admission weight.
        for _ in 0..2 {
            self.local.update(
                &self.queue,
                Vec3::ZERO,
                if enabled { &sources } else { &[] },
                0.25,
            );
        }
        let mut encoder = self.device.create_command_encoder(&Default::default());
        for index in self.local.faces_to_update() {
            let face = self.local.face(index);
            let mut pass = face.begin(&mut encoder);
            if let Some(actors) = &self.actors {
                assert!(actors.draw_shadow(&mut pass, &face.caster_group) > 0);
            }
            if let Some((vertices, cutout)) = caster {
                pass.set_pipeline(if cutout {
                    &self.caster_pipelines.1
                } else {
                    &self.caster_pipelines.0
                });
                pass.set_bind_group(0, &face.caster_group, &[]);
                pass.set_bind_group(1, &self.textures, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..6, 0, 0..1);
            }
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
fn gpu_point_shadows_cover_six_faces_edits_cutouts_and_source_exclusion() {
    verify_terrain(Fixture::new());
}

fn verify_terrain(mut scene: Fixture) {
    for direction in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
        scene.direction = direction;
        scene.floor = Fixture::quad(
            &scene.device,
            direction,
            4.0,
            2.5,
            0.0,
            2.0,
            [0.25; 2],
            1.0,
            1.0,
        );
        let opaque = Fixture::quad(
            &scene.device,
            direction,
            2.0,
            0.5,
            0.0,
            2.0,
            [0.25; 2],
            1.0,
            0.0,
        );
        let moved = Fixture::quad(
            &scene.device,
            direction,
            2.0,
            0.5,
            0.8,
            2.0,
            [0.25; 2],
            1.0,
            0.0,
        );
        let clear = scene.render(None, true);
        let shadow = scene.render(Some((&opaque, false)), true);
        let darkened = clear
            .chunks_exact(4)
            .zip(shadow.chunks_exact(4))
            .filter(|(a, b)| u16::from(a[0]) > u16::from(b[0]) + 1)
            .count();
        assert!(
            darkened > 100,
            "face {direction:?} failed to project opaque geometry: {darkened} pixels"
        );
        assert!(
            clear.iter().zip(&shadow).all(|(a, b)| a >= b),
            "occlusion cannot add energy"
        );
        assert_ne!(
            shadow,
            scene.render(Some((&moved, false)), true),
            "moving geometry must move the projected shadow"
        );
        assert_eq!(
            clear,
            scene.render(None, true),
            "removed geometry must not leave stale depth"
        );
        assert_eq!(
            clear,
            scene.render(Some((&opaque, false)), false),
            "disabled local maps preserve the original light"
        );
        let source_shell = Fixture::quad(
            &scene.device,
            direction,
            0.5,
            0.5,
            0.0,
            2.0,
            [0.25; 2],
            1.0,
            0.0,
        );
        assert_eq!(
            clear,
            scene.render(Some((&source_shell, false)), true),
            "the source block must not occlude its own light"
        );
        scene.floor = Fixture::quad(
            &scene.device,
            direction,
            4.0,
            2.5,
            0.0,
            2.0,
            [0.25; 2],
            1.0,
            0.0,
        );
        assert_eq!(
            scene.render(None, true),
            scene.render(Some((&opaque, false)), true),
            "local depth must not remove sky, sunlight or either baked bounce component"
        );
    }
    scene.direction = -Vec3::Y;
    scene.floor = Fixture::quad(
        &scene.device,
        scene.direction,
        4.0,
        2.5,
        0.0,
        2.0,
        [0.25; 2],
        1.0,
        1.0,
    );
    let clear = scene.render(None, true);
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
        let card = Fixture::quad(
            &scene.device,
            scene.direction,
            2.0,
            0.5,
            0.0,
            12.0,
            uv,
            1.0,
            0.0,
        );
        let image = scene.render(Some((&card, true)), true);
        if transparent {
            assert_eq!(clear, image, "alpha holes must transmit local light");
        } else {
            assert_ne!(
                clear, image,
                "opaque foliage texels must cast local shadows"
            );
            let moved = Fixture::quad(
                &scene.device,
                scene.direction,
                2.0,
                0.5,
                0.8,
                12.0,
                uv,
                1.0,
                0.0,
            );
            assert_ne!(
                image,
                scene.render(Some((&moved, true)), true),
                "edited foliage must move the local shadow"
            );
            assert_eq!(
                clear,
                scene.render(None, true),
                "removed foliage must clear local depth"
            );
        }
    }
}

/// Keep authored test assets private to avatars while exercising their actual
/// depth pipeline and a production terrain receiver in one scene.
pub(in crate::render) fn verify_actor_projection(
    catalog: &crate::content::Catalog,
    mut actor: crate::render::avatars::VisualAvatar,
) {
    let mut scene = Fixture::new();
    let mut renderer = crate::render::avatars::AvatarRenderer::new(
        &scene.device,
        &scene.queue,
        wgpu::TextureFormat::Rgba8Unorm,
        &scene.camera,
        catalog,
    );
    for direction in [-Vec3::Y, Vec3::Z] {
        scene.direction = direction;
        scene.floor = Fixture::quad(
            &scene.device,
            direction,
            4.0,
            2.5,
            0.0,
            2.0,
            [0.25; 2],
            1.0,
            1.0,
        );
        let clear = scene.render(None, true);
        actor.position = direction * 2.0
            + if direction == Vec3::Z {
                -Vec3::Y * 0.7
            } else {
                Vec3::ZERO
            };
        renderer.set(&scene.queue, &[actor]);
        scene.actors = Some(renderer);
        let shadow = scene.render(None, true);
        let darkened = clear
            .chunks_exact(4)
            .zip(shadow.chunks_exact(4))
            .filter(|(a, b)| a[0] > b[0].saturating_add(2))
            .count();
        assert!(
            darkened > 10,
            "{:?} must cast onto {direction:?}: {darkened} pixels",
            actor.model
        );
        actor.position.x += 0.7;
        scene.actors.as_mut().unwrap().set(&scene.queue, &[actor]);
        assert_ne!(
            shadow,
            scene.render(None, true),
            "actor motion must move the projected shadow"
        );
        assert_eq!(
            clear,
            scene.render(None, false),
            "disabled maps must restore actor-occluded light"
        );
        renderer = scene.actors.take().unwrap();
        assert_eq!(
            clear,
            scene.render(None, true),
            "removed actor must not leave stale depth"
        );
    }
}

#[test]
fn gpu_gl_point_shadow_depth_array_and_six_face_receivers() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
        eprintln!("GL adapter unavailable; optional local shadow validation skipped");
        return;
    };
    assert_eq!(adapter.get_info().backend, wgpu::Backend::Gl);
    verify_terrain(Fixture::from_adapter(adapter));
}

#[test]
fn gpu_unrelated_selected_source_cannot_shadow_untracked_local_color() {
    let mut scene = Fixture::new();
    let card = Fixture::quad(
        &scene.device,
        scene.direction,
        2.0,
        0.5,
        0.0,
        2.0,
        [0.25; 2],
        1.0,
        0.0,
    );
    scene.source_color = [0.0, 0.0, 1.0];
    let clear = scene.render(None, true);
    assert_eq!(
        clear,
        scene.render(Some((&card, false)), true),
        "a selected blue emitter must not shadow untracked orange radiance"
    );
    scene.source_color = [1.0, 0.63, 0.31];
    assert_ne!(
        clear,
        scene.render(Some((&card, false)), true),
        "the matching source must still cast"
    );
}
