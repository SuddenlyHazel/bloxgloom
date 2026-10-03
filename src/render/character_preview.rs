//! In-menu offscreen model preview using the same production skinning renderer.
use crate::{
    content::Catalog,
    render::{AvatarModel, AvatarRenderer, VisualAvatar},
    ui::CharacterPanel,
};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub(crate) struct CharacterPreview {
    renderer: AvatarRenderer,
    player_clips: std::collections::HashMap<u32, [Option<u16>; 6]>,
    color: wgpu::Texture,
    sampled_texture: wgpu::Texture,
    depth: wgpu::TextureView,
    pub(crate) sampled: wgpu::TextureView,
}

impl CharacterPreview {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, catalog: &Catalog) -> Self {
        let matrix = glam::camera::rh::proj::directx::orthographic(-0.8, 0.8, -0.1, 2.3, 0.1, 10.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 4.0), Vec3::ZERO, Vec3::Y);
        let camera = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("character menu camera"),
            contents: bytemuck::cast_slice(
                &super::daylight::Atmosphere::at(crate::daylight::INITIAL_MS)
                    .camera_data(matrix, Vec3::new(0.0, 0.0, 4.0)),
            ),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let renderer = AvatarRenderer::new(
            device,
            queue,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &camera,
            catalog,
        );
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("character menu portrait"),
            size: wgpu::Extent3d {
                width: 256,
                height: 384,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // A byte copy preserves sRGB-encoded pixels for egui's Unorm contract,
        // without requiring VIEW_FORMATS (unavailable on GLES software adapters).
        let sampled_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("character menu sampled portrait"),
            size: wgpu::Extent3d {
                width: 256,
                height: 384,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampled = sampled_texture.create_view(&Default::default());
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("character menu depth"),
            size: wgpu::Extent3d {
                width: 256,
                height: 384,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let player_clips = catalog
            .models()
            .filter_map(|(id, model)| {
                let p = model.player.as_ref()?;
                let index = |name: &Option<String>| {
                    name.as_ref().and_then(|name| {
                        model
                            .model
                            .clips
                            .iter()
                            .position(|c| &c.name == name)
                            .map(|i| i as u16)
                    })
                };
                Some((
                    id,
                    [
                        index(&p.idle),
                        index(&p.walk),
                        index(&p.crouch),
                        index(&p.tool_left),
                        index(&p.tool_right),
                        index(&p.run),
                    ],
                ))
            })
            .collect();
        Self {
            renderer,
            player_clips,
            color,
            sampled_texture,
            sampled,
            depth: depth.create_view(&Default::default()),
        }
    }

    pub(crate) fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        panel: CharacterPanel,
    ) {
        let clips = [
            "idle",
            "walk",
            "crouch",
            "tool_use_left",
            "tool_use_right",
            "run",
        ];
        self.renderer
            .preview_character_clip(clips[usize::from(panel.clip.min(5))], panel.time);
        let mut visual = panel.packaged.map(|p| p.visual);
        if let Some(packaged) = panel.packaged
            && let Some(state) = &mut visual
            && let Some(clips) = self.player_clips.get(&packaged.model)
            && let Some(clip) = clips[panel.clip.min(5) as usize].or(clips[0])
        {
            state.sample_tick = (panel.time.max(0.0) as f64 * 50.0) as u64;
            state.sequence = (panel.time.max(0.0) as f64 * 1000.0) as u32;
            state.playback = Some(bloxgloom_host_api::entity::ClipPlayback {
                clip,
                speed: 1.0,
                looping: true,
                crossfade_s: 0.0,
                started_tick: 0,
                sequence: state.sequence,
            });
        }
        self.renderer.set(
            queue,
            &[VisualAvatar {
                animation: Default::default(),
                model: panel.packaged.map_or(AvatarModel::Player, |p| {
                    AvatarModel::PackagedPlayer(p.model)
                }),
                pose: [-0.25, 0.0, 0.0, 0.0],
                motion: None,
                model_pose: visual,
                character_pose: [0.0; 4],
                character_look: [0.0; 2],
                character_crouch: 0.0,
                character_tool: None,
                character_recipe: panel.recipe,
                airborne: false,
                id: 0,
                position: Vec3::ZERO,
                cosmetics: panel.cosmetics,
                light_levels: [15, 0, 0, 0],
                bounce: [0; 4],
                glow_bounce: [0; 4],
                tint: [1.0; 3],
            }],
        );
        let color_view = self.color.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("character menu preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.055,
                        g: 0.075,
                        b: 0.07,
                        a: 1.0,
                    }),
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
        self.renderer.draw(&mut pass);
        drop(pass);
        encoder.copy_texture_to_texture(
            self.color.as_image_copy(),
            self.sampled_texture.as_image_copy(),
            wgpu::Extent3d {
                width: 256,
                height: 384,
                depth_or_array_layers: 1,
            },
        );
    }
}
