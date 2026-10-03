//! GPU resources and frame preparation for the UI overlay.

use super::{
    draw::{
        FONT_HEIGHT, FONT_WIDTH, MAX_UI_VERTICES, UI_SHADER, UiBuilder, UiVertex, make_font_atlas,
    },
    layout::{UiLayout, effective_ui_scale},
    types::{UiControl, UiDebug, UiFrame, UiScreen, UiSettings},
};
use crate::content::Catalog;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[derive(Clone, PartialEq)]
struct UiCacheKey {
    width: u32,
    height: u32,
    scale_bits: u32,
    screen: UiScreen,
    selected_slot: usize,
    inventory: [Option<crate::inventory::Stack>; crate::inventory::SLOTS],
    inventory_source: Option<u8>,
    kiln: Option<crate::protocol::workstation::WorkstationView>,
    container_screen: Option<Arc<bloxgloom_host_api::InventoryScreen>>,
    action_panel: Option<bloxgloom_host_api::actions::Panel>,
    kiln_source: Option<u8>,
    admin_enabled: bool,
    flying: bool,
    flying_pending: bool,
    admin_page: usize,
    debug: Option<UiDebug>,
    settings: UiSettings,
    hovered: Option<UiControl>,
}

/// The UI uses one alpha blended draw call and a static 8x8 ASCII font atlas.
pub(crate) struct UiRenderer {
    catalog: Arc<Catalog>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    bind_layout: wgpu::BindGroupLayout,
    package_bind_group: Option<wgpu::BindGroup>,
    package_start: usize,
    vertex_buffer: wgpu::Buffer,
    vertices: Vec<UiVertex>,
    layout: Option<UiLayout>,
    cache_key: Option<UiCacheKey>,
    cached_status: Option<String>,
    cached_admin_input: String,
    cached_inventory_search: String,
    width: u32,
    height: u32,
}

impl UiRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_catalog(device, queue, format, Arc::new(Catalog::builtins()))
    }

    pub(crate) fn new_with_catalog(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        catalog: Arc<Catalog>,
    ) -> Self {
        let atlas_bytes = make_font_atlas();
        let atlas = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("UI bitmap font atlas"),
                size: wgpu::Extent3d {
                    width: FONT_WIDTH,
                    height: FONT_HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &atlas_bytes,
        );
        let atlas_view = atlas.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("UI bitmap font sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("UI font layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("UI font bind group"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("screen-space UI shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("UI pipeline layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let attributes = wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x2,
            2 => Float32x4,
            3 => Float32
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("screen-space UI pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamic UI vertices"),
            size: (MAX_UI_VERTICES * std::mem::size_of::<UiVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            catalog,
            pipeline,
            bind_group,
            bind_layout,
            package_bind_group: None,
            package_start: 0,
            vertex_buffer,
            vertices: Vec::with_capacity(MAX_UI_VERTICES),
            layout: None,
            cache_key: None,
            cached_status: None,
            cached_admin_input: String::new(),
            cached_inventory_search: String::new(),
            width: 1,
            height: 1,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        frame: &UiFrame<'_>,
    ) {
        self.width = width.max(1);
        self.height = height.max(1);
        let scale = effective_ui_scale(self.width, self.height, frame.settings.scale);
        let key = UiCacheKey {
            width: self.width,
            height: self.height,
            scale_bits: scale.to_bits(),
            screen: frame.screen,
            selected_slot: frame.selected_slot,
            inventory: frame.inventory.clone(),
            inventory_source: frame.inventory_source,
            kiln: frame.kiln.clone(),
            container_screen: frame.container_screen.clone(),
            action_panel: frame.action_panel.clone(),
            kiln_source: frame.kiln_source,
            admin_enabled: frame.admin_enabled,
            flying: frame.flying,
            flying_pending: frame.flying_pending,
            admin_page: frame.admin_page,
            debug: frame.debug,
            settings: frame.settings,
            hovered: frame.hovered,
        };
        if frame.screen != UiScreen::Package
            && self.cache_key.as_ref() == Some(&key)
            && self.cached_status.as_deref() == frame.status
            && self.cached_admin_input == frame.admin_input
            && self.cached_inventory_search == frame.inventory_search
        {
            return;
        }
        let rebuild_layout = self.cache_key.as_ref().is_none_or(|old| {
            old.container_screen != key.container_screen || old.action_panel != key.action_panel
        }) || self.layout.as_ref().is_none_or(|layout| {
            layout.width != self.width
                || layout.height != self.height
                || layout.scale.to_bits() != scale.to_bits()
                || layout.screen != frame.screen
        });
        if rebuild_layout {
            self.layout = Some(
                UiLayout::new(self.width, self.height, scale, frame.screen)
                    .with_container(frame.container_screen.as_deref())
                    .with_actions(frame.action_panel.as_ref()),
            );
        }
        let layout = self.layout.as_ref().unwrap();
        self.vertices.clear();
        let mut builder = UiBuilder {
            vertices: &mut self.vertices,
            width: self.width as f32,
            height: self.height as f32,
            scale,
        };
        builder.draw_frame(frame, layout, &self.catalog);
        if frame.screen == UiScreen::Package
            && let Some(session) = frame.package_ui
        {
            session.draw_chrome(&mut builder);
        }
        self.package_start = builder.vertices.len();
        if frame.screen == UiScreen::Package
            && self.package_bind_group.is_some()
            && let Some(session) = frame.package_ui
        {
            session.draw(&mut builder);
        }
        if self.vertices.len() > MAX_UI_VERTICES {
            self.vertices
                .truncate(MAX_UI_VERTICES - MAX_UI_VERTICES % 6);
        }
        self.package_start = self.package_start.min(self.vertices.len());
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
        self.cache_key = Some(key);
        self.cached_status = frame.status.map(str::to_owned);
        if self.cached_admin_input != frame.admin_input {
            self.cached_admin_input.clear();
            self.cached_admin_input.push_str(frame.admin_input);
        }
        if self.cached_inventory_search != frame.inventory_search {
            self.cached_inventory_search.clear();
            self.cached_inventory_search
                .push_str(frame.inventory_search);
        }
    }

    pub(crate) fn encode<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.vertices.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.package_start as u32, 0..1);
        if let Some(group) = &self.package_bind_group
            && self.package_start < self.vertices.len()
        {
            pass.set_bind_group(0, group, &[]);
            pass.draw(self.package_start as u32..self.vertices.len() as u32, 0..1);
        }
    }

    pub(crate) fn install_package_ui(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resources: &super::authored::Resources,
    ) {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("verified package UI atlas"),
                size: wgpu::Extent3d {
                    width: super::authored::ATLAS_SIZE as u32,
                    height: super::authored::ATLAS_SIZE as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &resources.pixels,
        );
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        self.package_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("verified package UI"),
            layout: &self.bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));
        self.cache_key = None;
    }
}
