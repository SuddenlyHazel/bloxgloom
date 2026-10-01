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
        Self {
            renderer,
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
        let clips = ["idle", "walk", "crouch", "tool_use_left", "tool_use_right"];
        self.renderer
            .preview_character_clip(clips[usize::from(panel.clip.min(4))], panel.time);
        self.renderer.set(
            queue,
            &[VisualAvatar {
                animation: Default::default(),
                model: AvatarModel::Player,
                pose: [-0.25, 0.0, 0.0, 0.0],
                motion: None,
                character_pose: [0.0; 3],
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
