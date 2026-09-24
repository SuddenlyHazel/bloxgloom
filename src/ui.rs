//! Screen-space UI data, hit testing, and the compact GPU overlay renderer.

use bytemuck::{Pod, Zeroable};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use wgpu::util::DeviceExt;

const MAX_UI_VERTICES: usize = 32_768;
const FONT_WIDTH: u32 = 128;
const FONT_HEIGHT: u32 = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiScreen {
    #[default]
    Playing,
    Inventory,
    Pause,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SettingId {
    Sensitivity,
    FieldOfView,
    ViewDistance,
    UiScale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiControl {
    HotbarSlot(u8),
    CatalogBlock(u8),
    Resume,
    OpenSettings,
    Exit,
    Back,
    Decrease(SettingId),
    Increase(SettingId),
    ToggleFullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSettings {
    /// Mouse radians per physical pixel.
    pub sensitivity: f32,
    pub fov_degrees: f32,
    pub view_distance: u8,
    pub scale: f32,
    pub fullscreen: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            sensitivity: 0.002,
            fov_degrees: 70.0,
            view_distance: 3,
            scale: 1.0,
            fullscreen: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiDebug {
    pub position: [f32; 3],
    pub fps: f32,
    pub frame_ms: f32,
    pub visible_chunks: usize,
    pub cached_chunks: usize,
    pub latency_ms: Option<u32>,
}

/// Values needed to draw a frame. Borrow status text to avoid per-frame string allocation.
#[derive(Clone, Copy, Debug)]
pub struct UiFrame<'a> {
    pub screen: UiScreen,
    pub selected_slot: usize,
    pub hotbar: [u8; 9],
    pub target: Option<[i32; 3]>,
    pub status: Option<&'a str>,
    pub debug: Option<UiDebug>,
    pub catalog_selection: u8,
    pub settings: UiSettings,
    pub hovered: Option<UiControl>,
}

impl Default for UiFrame<'_> {
    fn default() -> Self {
        Self {
            screen: UiScreen::Playing,
            selected_slot: 0,
            hotbar: [2, 1, 2, 3, 1, 2, 3, 1, 2],
            target: None,
            status: None,
            debug: None,
            catalog_selection: 2,
            settings: UiSettings::default(),
            hovered: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

#[derive(Clone, Debug)]
struct HitRect {
    control: UiControl,
    rect: UiRect,
}

/// Physical-pixel rectangles for all controls, shared by rendering and input handling.
#[derive(Clone, Debug)]
pub struct UiLayout {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    screen: UiScreen,
    hits: Vec<HitRect>,
}

impl UiLayout {
    pub fn new(width: u32, height: u32, scale: f32, screen: UiScreen) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let scale = if scale.is_finite() {
            scale.clamp(0.75, 2.0)
        } else {
            1.0
        };
        let mut layout = Self {
            width,
            height,
            scale,
            screen,
            hits: Vec::with_capacity(20),
        };
        layout.add_hotbar();
        match screen {
            UiScreen::Playing => {}
            UiScreen::Inventory => layout.add_inventory(),
            UiScreen::Pause => layout.add_pause(),
            UiScreen::Settings => layout.add_settings(),
        }
        layout
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<UiControl> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(x, y))
            .map(|hit| hit.control)
    }

    pub fn rect(&self, control: UiControl) -> Option<UiRect> {
        self.hits
            .iter()
            .find(|hit| hit.control == control)
            .map(|hit| hit.rect)
    }

    fn push(&mut self, control: UiControl, rect: UiRect) {
        self.hits.push(HitRect { control, rect });
    }

    fn add_hotbar(&mut self) {
        let gap = (6.0 * self.scale).max(3.0);
        let proposed = 54.0 * self.scale;
        let slot = proposed.min((self.width as f32 - 32.0).max(9.0) / 9.0 - gap);
        let slot = slot.max(24.0);
        let total = slot * 9.0 + gap * 8.0;
        let x0 = (self.width as f32 - total) * 0.5;
        let y = (self.height as f32 - slot - 22.0 * self.scale).max(4.0);
        for slot_index in 0..9 {
            self.push(
                UiControl::HotbarSlot(slot_index),
                UiRect {
                    x: x0 + slot_index as f32 * (slot + gap),
                    y,
                    width: slot,
                    height: slot,
                },
            );
        }
    }

    fn add_inventory(&mut self) {
        let panel = self.inventory_panel();
        let gap = 14.0 * self.scale;
        let inner_gap = 12.0 * self.scale;
        let card_width = ((panel.width - gap * 2.0 - inner_gap * 2.0) / 3.0).max(44.0);
        let compact = panel.height < 300.0 * self.scale;
        let card_height = (panel.height * 0.34).clamp(76.0 * self.scale, 138.0 * self.scale);
        let card_y = panel.y + (if compact { 78.0 } else { 98.0 }) * self.scale;
        let start_x = panel.x + gap;
        for (index, block) in [1u8, 2, 3].into_iter().enumerate() {
            self.push(
                UiControl::CatalogBlock(block),
                UiRect {
                    x: start_x + index as f32 * (card_width + inner_gap),
                    y: card_y,
                    width: card_width,
                    height: card_height,
                },
            );
        }
    }

    fn add_pause(&mut self) {
        let panel = self.pause_panel();
        let button_width = panel.width * 0.72;
        let compact = panel.height < 360.0 * self.scale;
        let button_height = if compact {
            38.0 * self.scale
        } else {
            (48.0 * self.scale).clamp(36.0, 56.0)
        };
        let gap = if compact {
            8.0 * self.scale
        } else {
            14.0 * self.scale
        };
        let first_y = if compact {
            panel.y + 86.0 * self.scale
        } else {
            panel.y + panel.height * 0.38
        };
        let x = panel.x + (panel.width - button_width) * 0.5;
        for (index, control) in [UiControl::Resume, UiControl::OpenSettings, UiControl::Exit]
            .into_iter()
            .enumerate()
        {
            self.push(
                control,
                UiRect {
                    x,
                    y: first_y + index as f32 * (button_height + gap),
                    width: button_width,
                    height: button_height,
                },
            );
        }
    }

    fn add_settings(&mut self) {
        let panel = self.settings_panel();
        let compact = panel.height < 500.0 * self.scale;
        let row_height = if compact {
            40.0 * self.scale
        } else {
            (52.0 * self.scale).clamp(42.0, 58.0)
        };
        let top = panel.y + (if compact { 74.0 } else { 116.0 }) * self.scale;
        let label_width = panel.width * if compact { 0.46 } else { 0.49 };
        let control_x = panel.x + label_width;
        let button_width = (42.0 * self.scale).clamp(34.0, 48.0);
        let value_width = if compact {
            72.0 * self.scale
        } else {
            (106.0 * self.scale).clamp(72.0, 128.0)
        };
        for (index, setting) in [
            SettingId::Sensitivity,
            SettingId::FieldOfView,
            SettingId::ViewDistance,
            SettingId::UiScale,
        ]
        .into_iter()
        .enumerate()
        {
            let y = top + index as f32 * row_height;
            self.push(
                UiControl::Decrease(setting),
                UiRect {
                    x: control_x,
                    y,
                    width: button_width,
                    height: row_height - 8.0 * self.scale,
                },
            );
            self.push(
                UiControl::Increase(setting),
                UiRect {
                    x: control_x + button_width + value_width,
                    y,
                    width: button_width,
                    height: row_height - 8.0 * self.scale,
                },
            );
        }
        self.push(
            UiControl::ToggleFullscreen,
            UiRect {
                x: panel.x + panel.width * if compact { 0.45 } else { 0.22 },
                y: panel.y + panel.height - (if compact { 46.0 } else { 104.0 }) * self.scale,
                width: panel.width * if compact { 0.52 } else { 0.56 },
                height: if compact {
                    32.0 * self.scale
                } else {
                    (48.0 * self.scale).clamp(36.0, 56.0)
                },
            },
        );
        self.push(
            UiControl::Back,
            UiRect {
                x: panel.x + 18.0 * self.scale,
                y: panel.y + panel.height - (if compact { 46.0 } else { 58.0 }) * self.scale,
                width: if compact {
                    panel.width * 0.34
                } else {
                    (116.0 * self.scale).clamp(88.0, 142.0)
                },
                height: if compact {
                    32.0 * self.scale
                } else {
                    (38.0 * self.scale).clamp(30.0, 44.0)
                },
            },
        );
    }

    fn inventory_panel(&self) -> UiRect {
        let desired_height = if self.height < 500 { 208.0 } else { 420.0 };
        centered_panel(
            self.width,
            self.height,
            760.0 * self.scale,
            desired_height * self.scale,
            14.0 * self.scale,
        )
    }

    fn pause_panel(&self) -> UiRect {
        let desired_height = if self.height < 500 { 280.0 } else { 420.0 };
        centered_panel(
            self.width,
            self.height,
            420.0 * self.scale,
            desired_height * self.scale,
            14.0 * self.scale,
        )
    }

    fn settings_panel(&self) -> UiRect {
        centered_panel(
            self.width,
            self.height,
            720.0 * self.scale,
            540.0 * self.scale,
            14.0,
        )
    }
}

fn centered_panel(
    width: u32,
    height: u32,
    desired_width: f32,
    desired_height: f32,
    margin: f32,
) -> UiRect {
    let panel_width = desired_width.min((width as f32 - margin * 2.0).max(1.0));
    let panel_height = desired_height.min((height as f32 - margin * 2.0).max(1.0));
    UiRect {
        x: (width as f32 - panel_width) * 0.5,
        y: (height as f32 - panel_height) * 0.5,
        width: panel_width,
        height: panel_height,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    textured: f32,
}

#[derive(Clone, Copy, PartialEq)]
struct UiCacheKey {
    width: u32,
    height: u32,
    scale_bits: u32,
    screen: UiScreen,
    selected_slot: usize,
    hotbar: [u8; 9],
    debug: Option<UiDebug>,
    catalog_selection: u8,
    settings: UiSettings,
    hovered: Option<UiControl>,
}

/// The UI uses one alpha blended draw call and a static 8x8 ASCII font atlas.
pub(crate) struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    vertices: Vec<UiVertex>,
    layout: Option<UiLayout>,
    cache_key: Option<UiCacheKey>,
    cached_status: Option<String>,
    width: u32,
    height: u32,
}

impl UiRenderer {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
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
            pipeline,
            bind_group,
            vertex_buffer,
            vertices: Vec::with_capacity(MAX_UI_VERTICES),
            layout: None,
            cache_key: None,
            cached_status: None,
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
        let scale = frame.settings.scale.clamp(0.75, 2.0);
        let key = UiCacheKey {
            width: self.width,
            height: self.height,
            scale_bits: scale.to_bits(),
            screen: frame.screen,
            selected_slot: frame.selected_slot,
            hotbar: frame.hotbar,
            debug: frame.debug,
            catalog_selection: frame.catalog_selection,
            settings: frame.settings,
            hovered: frame.hovered,
        };
        if self.cache_key == Some(key) && self.cached_status.as_deref() == frame.status {
            return;
        }
        let rebuild_layout = self.layout.as_ref().is_none_or(|layout| {
            layout.width != self.width
                || layout.height != self.height
                || layout.scale.to_bits() != scale.to_bits()
                || layout.screen != frame.screen
        });
        if rebuild_layout {
            self.layout = Some(UiLayout::new(self.width, self.height, scale, frame.screen));
        }
        let layout = self.layout.as_ref().unwrap();
        self.vertices.clear();
        let mut builder = UiBuilder {
            vertices: &mut self.vertices,
            width: self.width as f32,
            height: self.height as f32,
            scale,
        };
        builder.draw_frame(frame, layout);
        if self.vertices.len() > MAX_UI_VERTICES {
            self.vertices
                .truncate(MAX_UI_VERTICES - MAX_UI_VERTICES % 6);
        }
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
        }
        self.cache_key = Some(key);
        self.cached_status = frame.status.map(str::to_owned);
    }

    pub(crate) fn encode<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.vertices.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertices.len() as u32, 0..1);
    }
}

struct UiBuilder<'a> {
    vertices: &'a mut Vec<UiVertex>,
    width: f32,
    height: f32,
    scale: f32,
}

impl UiBuilder<'_> {
    fn draw_frame(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.draw_hud(frame, layout);
        match frame.screen {
            UiScreen::Playing => {}
            UiScreen::Inventory => self.draw_inventory(frame, layout),
            UiScreen::Pause => self.draw_pause(frame, layout),
            UiScreen::Settings => self.draw_settings(frame, layout),
        }
        if let Some(debug) = frame.debug {
            self.draw_debug(debug);
        }
    }

    fn draw_hud(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        let center_x = self.width * 0.5;
        let center_y = self.height * 0.5;
        if frame.screen == UiScreen::Playing {
            let gap = 4.0 * self.scale;
            let arm = 8.0 * self.scale;
            let thickness = (2.0 * self.scale).max(1.0);
            let shadow = [0.03, 0.04, 0.03, 0.78];
            let white = [0.95, 0.98, 0.91, 0.98];
            // Dark edging keeps the small crosshair visible on bright sky and leaves.
            for (x, y, w, h) in [
                (
                    center_x - gap - arm - 1.0,
                    center_y - 1.0,
                    arm,
                    thickness + 2.0,
                ),
                (center_x + gap + 1.0, center_y - 1.0, arm, thickness + 2.0),
                (
                    center_x - 1.0,
                    center_y - gap - arm - 1.0,
                    thickness + 2.0,
                    arm,
                ),
                (center_x - 1.0, center_y + gap + 1.0, thickness + 2.0, arm),
            ] {
                self.rect(x, y, w, h, shadow);
            }
            for (x, y, w, h) in [
                (
                    center_x - gap - arm,
                    center_y - thickness * 0.5,
                    arm,
                    thickness,
                ),
                (center_x + gap, center_y - thickness * 0.5, arm, thickness),
                (
                    center_x - thickness * 0.5,
                    center_y - gap - arm,
                    thickness,
                    arm,
                ),
                (center_x - thickness * 0.5, center_y + gap, thickness, arm),
            ] {
                self.rect(x, y, w, h, white);
            }
        }
        if matches!(frame.screen, UiScreen::Playing | UiScreen::Inventory) {
            for index in 0usize..9 {
                let Some(rect) = layout.rect(UiControl::HotbarSlot(index as u8)) else {
                    continue;
                };
                let selected = index == frame.selected_slot.min(8);
                let bg = if selected {
                    [0.09, 0.12, 0.10, 0.92]
                } else {
                    [0.04, 0.055, 0.06, 0.76]
                };
                self.rounded_panel(
                    rect,
                    bg,
                    if selected {
                        GOLD
                    } else {
                        [0.30, 0.34, 0.31, 0.88]
                    },
                    1.5 * self.scale,
                );
                let swatch = inset(rect, 10.0 * self.scale, 10.0 * self.scale);
                self.rect(
                    swatch.x,
                    swatch.y,
                    swatch.width,
                    swatch.height,
                    block_color(frame.hotbar[index]),
                );
                let number = [b'1' + index as u8];
                self.text(
                    std::str::from_utf8(&number).unwrap_or("?"),
                    rect.x + 5.0 * self.scale,
                    rect.y + 4.0 * self.scale,
                    0.72,
                    if selected { GOLD } else { MUTED },
                    1,
                );
            }
            let selected_block = frame.hotbar[frame.selected_slot.min(8)];
            let label_y = layout
                .rect(UiControl::HotbarSlot(0))
                .map_or(self.height - 86.0 * self.scale, |r| r.y - 29.0 * self.scale);
            self.center_text(
                block_name(selected_block),
                self.width * 0.5,
                label_y,
                0.96,
                TEXT,
            );
        }
        if frame.screen == UiScreen::Playing
            && let Some(status) = frame.status.filter(|s| !s.is_empty())
        {
            let max_chars =
                ((self.width - 24.0 * self.scale) / (18.0 * self.scale * 0.82)).max(1.0) as usize;
            let shown_chars = status.chars().count().min(max_chars).min(96);
            let shown_width = shown_chars as f32 * 18.0 * self.scale * 0.82;
            self.text(
                status,
                self.width - shown_width - 12.0 * self.scale,
                12.0 * self.scale,
                0.82,
                [0.95, 0.79, 0.47, 1.0],
                max_chars.min(96),
            );
        }
    }

    fn draw_inventory(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.screen_dim();
        let panel = layout.inventory_panel();
        self.panel(panel);
        let compact = panel.width < 700.0 * self.scale;
        self.text(
            "BLOCK CATALOG",
            panel.x + (if compact { 20.0 } else { 30.0 }) * self.scale,
            panel.y + (if compact { 14.0 } else { 28.0 }) * self.scale,
            if compact { 0.96 } else { 1.2 },
            TEXT,
            32,
        );
        let current_slot = frame.selected_slot.min(8) + 1;
        let hint = format!("CHOOSE A BLOCK FOR SLOT {current_slot}");
        if compact {
            self.text(
                &hint,
                panel.x + 20.0 * self.scale,
                panel.y + 46.0 * self.scale,
                0.66,
                GOLD,
                40,
            );
        } else {
            self.text(
                "CREATIVE BUILDING",
                panel.x + 30.0 * self.scale,
                panel.y + 60.0 * self.scale,
                0.76,
                MUTED,
                32,
            );
            self.text(
                &hint,
                panel.x + panel.width - 30.0 * self.scale - text_width(&hint, self.scale, 0.78),
                panel.y + 36.0 * self.scale,
                0.78,
                GOLD,
                40,
            );
        }
        for block in [1u8, 2, 3] {
            let Some(card) = layout.rect(UiControl::CatalogBlock(block)) else {
                continue;
            };
            let selected = frame.catalog_selection == block;
            let hovered = frame.hovered == Some(UiControl::CatalogBlock(block));
            self.rounded_panel(
                card,
                if selected {
                    [0.12, 0.17, 0.12, 0.95]
                } else {
                    [0.075, 0.09, 0.09, 0.92]
                },
                if selected {
                    GOLD
                } else if hovered {
                    [0.65, 0.77, 0.52, 0.9]
                } else {
                    EDGE
                },
                if selected {
                    2.2 * self.scale
                } else {
                    1.0 * self.scale
                },
            );
            let swatch_size = (42.0 * self.scale)
                .min(card.height * 0.44)
                .min(card.width * 0.35);
            let swatch = UiRect {
                x: card.x + 18.0 * self.scale,
                y: card.y + (card.height - swatch_size) * 0.5,
                width: swatch_size,
                height: swatch_size,
            };
            self.rect(
                swatch.x + 3.0 * self.scale,
                swatch.y,
                swatch.width,
                swatch.height,
                block_color(block),
            );
            self.rect(
                swatch.x,
                swatch.y + swatch.height * 0.65,
                swatch.width,
                swatch.height * 0.35,
                darken(block_color(block), 0.72),
            );
            self.text(
                block_name(block),
                swatch.x + swatch.width + 14.0 * self.scale,
                card.y + card.height * 0.44,
                1.0,
                TEXT,
                18,
            );
            self.text(
                "PLACEABLE",
                swatch.x + swatch.width + 14.0 * self.scale,
                card.y + card.height * 0.70,
                0.67,
                MUTED,
                18,
            );
        }
        let footer = "1-9 CHOOSES SLOT  /  E OR ESC CLOSES";
        self.center_text(
            footer,
            panel.x + panel.width * 0.5,
            panel.y + panel.height - (if compact { 32.0 } else { 46.0 }) * self.scale,
            if compact { 0.65 } else { 0.76 },
            MUTED,
        );
    }

    fn draw_pause(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.screen_dim();
        let panel = layout.pause_panel();
        self.panel(panel);
        let compact = panel.height < 360.0 * self.scale;
        self.center_text(
            "BLOXGLOOM",
            panel.x + panel.width * 0.5,
            panel.y + (if compact { 42.0 } else { 56.0 }) * self.scale,
            if compact { 1.2 } else { 1.55 },
            TEXT,
        );
        if !compact {
            self.center_text(
                "PAUSED",
                panel.x + panel.width * 0.5,
                panel.y + 92.0 * self.scale,
                0.82,
                MUTED,
            );
        }
        for (control, label) in [
            (UiControl::Resume, "RESUME"),
            (UiControl::OpenSettings, "SETTINGS"),
            (UiControl::Exit, "EXIT GAME"),
        ] {
            if let Some(rect) = layout.rect(control) {
                self.button(rect, label, frame.hovered == Some(control), false);
            }
        }
        self.center_text(
            "ESC TO RETURN",
            panel.x + panel.width * 0.5,
            panel.y + panel.height - (if compact { 22.0 } else { 30.0 }) * self.scale,
            if compact { 0.62 } else { 0.7 },
            MUTED,
        );
    }

    fn draw_settings(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.screen_dim();
        let panel = layout.settings_panel();
        self.panel(panel);
        let compact = panel.height < 500.0 * self.scale;
        self.text(
            "SETTINGS",
            panel.x + 30.0 * self.scale,
            panel.y + (if compact { 14.0 } else { 28.0 }) * self.scale,
            if compact { 1.0 } else { 1.3 },
            TEXT,
            30,
        );
        if !compact {
            self.text(
                "LOCAL CLIENT OPTIONS",
                panel.x + 30.0 * self.scale,
                panel.y + 62.0 * self.scale,
                0.72,
                MUTED,
                36,
            );
        }
        let rows = [
            (
                SettingId::Sensitivity,
                "MOUSE SENSITIVITY",
                format!("{:.3}", frame.settings.sensitivity),
            ),
            (
                SettingId::FieldOfView,
                "FIELD OF VIEW",
                format!("{:.0} DEG", frame.settings.fov_degrees),
            ),
            (
                SettingId::ViewDistance,
                "VIEW DISTANCE",
                frame.settings.view_distance.to_string(),
            ),
            (
                SettingId::UiScale,
                "UI SCALE",
                format!("{:.1}X", frame.settings.scale),
            ),
        ];
        let top = panel.y + (if compact { 74.0 } else { 116.0 }) * self.scale;
        let row_height = if compact {
            40.0 * self.scale
        } else {
            (52.0 * self.scale).clamp(42.0, 58.0)
        };
        let control_x = panel.x + panel.width * if compact { 0.46 } else { 0.49 };
        let minus_w = (42.0 * self.scale).clamp(34.0, 48.0);
        let value_w = if compact {
            72.0 * self.scale
        } else {
            (106.0 * self.scale).clamp(72.0, 128.0)
        };
        for (index, (setting, label, value)) in rows.iter().enumerate() {
            let y = top + index as f32 * row_height;
            self.text(
                label,
                panel.x + (if compact { 20.0 } else { 32.0 }) * self.scale,
                y + (if compact { 9.0 } else { 10.0 }) * self.scale,
                if compact { 0.68 } else { 0.82 },
                TEXT,
                24,
            );
            if let Some(rect) = layout.rect(UiControl::Decrease(*setting)) {
                self.button(
                    rect,
                    "-",
                    frame.hovered == Some(UiControl::Decrease(*setting)),
                    true,
                );
            }
            let value_rect = UiRect {
                x: control_x + minus_w,
                y,
                width: value_w,
                height: row_height - 8.0 * self.scale,
            };
            self.rounded_panel(
                value_rect,
                [0.035, 0.05, 0.05, 0.90],
                EDGE,
                1.0 * self.scale,
            );
            self.center_text(
                value,
                value_rect.x + value_rect.width * 0.5,
                value_rect.y + value_rect.height * 0.5,
                if compact { 0.60 } else { 0.78 },
                GOLD,
            );
            if let Some(rect) = layout.rect(UiControl::Increase(*setting)) {
                self.button(
                    rect,
                    "+",
                    frame.hovered == Some(UiControl::Increase(*setting)),
                    true,
                );
            }
        }
        if let Some(rect) = layout.rect(UiControl::ToggleFullscreen) {
            let label = if frame.settings.fullscreen {
                "FULLSCREEN: ON"
            } else {
                "FULLSCREEN: OFF"
            };
            self.button(
                rect,
                label,
                frame.hovered == Some(UiControl::ToggleFullscreen),
                compact,
            );
        }
        if let Some(rect) = layout.rect(UiControl::Back) {
            self.button(rect, "BACK", frame.hovered == Some(UiControl::Back), false);
        }
        if !compact {
            const HELP: &str = "ARROWS / +/- ADJUST  /  ESC BACK";
            self.text(
                HELP,
                panel.x + panel.width - 32.0 * self.scale - text_width(HELP, self.scale, 0.64),
                panel.y + panel.height - 46.0 * self.scale,
                0.64,
                MUTED,
                42,
            );
        }
    }

    fn draw_debug(&mut self, debug: UiDebug) {
        let scale = self.scale;
        let x = 12.0 * scale;
        let y = 12.0 * scale;
        let line_h = 19.0 * scale;
        let values = [
            format!(
                "XYZ  {:.1}  {:.1}  {:.1}",
                debug.position[0], debug.position[1], debug.position[2]
            ),
            format!("FPS  {:.0}  /  {:.2} MS", debug.fps, debug.frame_ms),
            format!(
                "CHUNKS  {} VISIBLE  /  {} CACHED",
                debug.visible_chunks, debug.cached_chunks
            ),
            debug
                .latency_ms
                .map_or_else(|| "PING  --".into(), |ms| format!("PING  {ms} MS")),
        ];
        let width = values
            .iter()
            .map(|line| text_width(line, scale, 0.76))
            .fold(0.0, f32::max)
            + 20.0 * scale;
        self.rounded_panel(
            UiRect {
                x: 7.0 * scale,
                y: 7.0 * scale,
                width,
                height: line_h * values.len() as f32 + 14.0 * scale,
            },
            [0.018, 0.027, 0.032, 0.8],
            [0.31, 0.43, 0.39, 0.72],
            scale,
        );
        for (index, line) in values.iter().enumerate() {
            self.text(
                line,
                x,
                y + index as f32 * line_h,
                0.76,
                [0.84, 0.91, 0.82, 0.97],
                96,
            );
        }
    }

    fn screen_dim(&mut self) {
        self.rect(
            0.0,
            0.0,
            self.width,
            self.height,
            [0.012, 0.02, 0.025, 0.69],
        );
    }

    fn panel(&mut self, rect: UiRect) {
        self.rounded_panel(
            rect,
            [0.035, 0.055, 0.056, 1.0],
            [0.28, 0.38, 0.32, 0.94],
            2.0 * self.scale,
        );
        self.rect(
            rect.x + 2.0 * self.scale,
            rect.y + 2.0 * self.scale,
            rect.width - 4.0 * self.scale,
            3.0 * self.scale,
            [0.49, 0.65, 0.36, 0.94],
        );
    }

    fn button(&mut self, rect: UiRect, label: &str, hovered: bool, compact: bool) {
        self.rounded_panel(
            rect,
            if hovered {
                [0.18, 0.25, 0.17, 0.98]
            } else {
                [0.075, 0.10, 0.095, 0.96]
            },
            if hovered {
                [0.72, 0.82, 0.47, 1.0]
            } else {
                EDGE
            },
            if hovered {
                2.0 * self.scale
            } else {
                1.0 * self.scale
            },
        );
        self.center_text(
            label,
            rect.x + rect.width * 0.5,
            rect.y + rect.height * 0.5,
            if compact { 0.84 } else { 0.9 },
            if hovered { GOLD } else { TEXT },
        );
    }

    fn rounded_panel(&mut self, rect: UiRect, fill: [f32; 4], border: [f32; 4], thickness: f32) {
        self.rect(rect.x, rect.y, rect.width, rect.height, fill);
        let t = thickness.max(1.0);
        self.rect(rect.x, rect.y, rect.width, t, border);
        self.rect(rect.x, rect.y + rect.height - t, rect.width, t, border);
        self.rect(rect.x, rect.y, t, rect.height, border);
        self.rect(rect.x + rect.width - t, rect.y, t, rect.height, border);
    }

    fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) {
        if width <= 0.0 || height <= 0.0 || self.vertices.len() + 6 > MAX_UI_VERTICES {
            return;
        }
        let x0 = x / self.width * 2.0 - 1.0;
        let x1 = (x + width) / self.width * 2.0 - 1.0;
        let y0 = 1.0 - y / self.height * 2.0;
        let y1 = 1.0 - (y + height) / self.height * 2.0;
        self.triangle(
            vertex([x0, y0], [0.0, 0.0], color, 0.0),
            vertex([x1, y0], [0.0, 0.0], color, 0.0),
            vertex([x1, y1], [0.0, 0.0], color, 0.0),
        );
        self.triangle(
            vertex([x0, y0], [0.0, 0.0], color, 0.0),
            vertex([x1, y1], [0.0, 0.0], color, 0.0),
            vertex([x0, y1], [0.0, 0.0], color, 0.0),
        );
    }

    fn text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        font_scale: f32,
        color: [f32; 4],
        max_chars: usize,
    ) {
        let cell = 16.0 * self.scale * font_scale;
        let advance = 18.0 * self.scale * font_scale;
        let max_chars = max_chars.min(128);
        for (index, ch) in text.chars().take(max_chars).enumerate() {
            let ch = if (ch as u32) < 128 { ch as u8 } else { b'?' };
            let column = u32::from(ch) % 16;
            let row = u32::from(ch) / 16;
            let uv0 = [column as f32 / 16.0, row as f32 / 8.0];
            let uv1 = [(column + 1) as f32 / 16.0, (row + 1) as f32 / 8.0];
            let left = x + index as f32 * advance;
            self.glyph(left, y, cell, cell, uv0, uv1, color);
        }
    }

    fn center_text(
        &mut self,
        text: &str,
        center_x: f32,
        center_y: f32,
        font_scale: f32,
        color: [f32; 4],
    ) {
        let len = text.chars().count().min(128);
        let advance = 18.0 * self.scale * font_scale;
        let width = len as f32 * advance - 2.0 * self.scale * font_scale;
        let height = 16.0 * self.scale * font_scale;
        self.text(
            text,
            center_x - width * 0.5,
            center_y - height * 0.5,
            font_scale,
            color,
            128,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn glyph(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    ) {
        if self.vertices.len() + 6 > MAX_UI_VERTICES {
            return;
        }
        let x0 = x / self.width * 2.0 - 1.0;
        let x1 = (x + width) / self.width * 2.0 - 1.0;
        let y0 = 1.0 - y / self.height * 2.0;
        let y1 = 1.0 - (y + height) / self.height * 2.0;
        self.triangle(
            vertex([x0, y0], uv0, color, 1.0),
            vertex([x1, y0], [uv1[0], uv0[1]], color, 1.0),
            vertex([x1, y1], uv1, color, 1.0),
        );
        self.triangle(
            vertex([x0, y0], uv0, color, 1.0),
            vertex([x1, y1], uv1, color, 1.0),
            vertex([x0, y1], [uv0[0], uv1[1]], color, 1.0),
        );
    }

    fn triangle(&mut self, a: UiVertex, b: UiVertex, c: UiVertex) {
        if self.vertices.len() + 3 <= MAX_UI_VERTICES {
            self.vertices.extend_from_slice(&[a, b, c]);
        }
    }
}

fn make_font_atlas() -> Vec<u8> {
    let mut pixels = vec![0u8; (FONT_WIDTH * FONT_HEIGHT * 4) as usize];
    for code in 0u8..=127 {
        let Some(glyph) = BASIC_FONTS.get(char::from(code)) else {
            continue;
        };
        let cell_x = u32::from(code) % 16 * 8;
        let cell_y = u32::from(code) / 16 * 8;
        for (y, row) in glyph.into_iter().enumerate() {
            for x in 0..8 {
                let alpha = if row & (1 << x) != 0 { 255 } else { 0 };
                let offset = (((cell_y + y as u32) * FONT_WIDTH + cell_x + x) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&[255, 255, 255, alpha]);
            }
        }
    }
    pixels
}

fn vertex(position: [f32; 2], uv: [f32; 2], color: [f32; 4], textured: f32) -> UiVertex {
    UiVertex {
        position,
        uv,
        color,
        textured,
    }
}

fn inset(rect: UiRect, x: f32, y: f32) -> UiRect {
    UiRect {
        x: rect.x + x,
        y: rect.y + y,
        width: (rect.width - x * 2.0).max(1.0),
        height: (rect.height - y * 2.0).max(1.0),
    }
}

fn block_name(block: u8) -> &'static str {
    match block {
        1 => "GRASS",
        2 => "DIRT",
        3 => "STONE",
        _ => "UNKNOWN",
    }
}

fn block_color(block: u8) -> [f32; 4] {
    match block {
        1 => [0.32, 0.62, 0.26, 1.0],
        2 => [0.52, 0.34, 0.21, 1.0],
        3 => [0.48, 0.52, 0.53, 1.0],
        _ => [0.6, 0.3, 0.8, 1.0],
    }
}

fn darken(mut color: [f32; 4], amount: f32) -> [f32; 4] {
    for channel in &mut color[..3] {
        *channel *= amount;
    }
    color
}

fn text_width(text: &str, scale: f32, font_scale: f32) -> f32 {
    text.chars().count().min(128) as f32 * 18.0 * scale * font_scale
}

const TEXT: [f32; 4] = [0.90, 0.94, 0.87, 1.0];
const MUTED: [f32; 4] = [0.61, 0.68, 0.63, 1.0];
const GOLD: [f32; 4] = [0.89, 0.72, 0.34, 1.0];
const EDGE: [f32; 4] = [0.28, 0.38, 0.32, 0.93];

const UI_SHADER: &str = r#"
struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) textured: f32,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) textured: f32,
};

@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var output: VertexOut;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    output.textured = input.textured;
    return output;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    if input.textured > 0.5 {
        let alpha = textureSample(font_atlas, font_sampler, input.uv).a;
        return vec4<f32>(input.color.rgb, input.color.a * alpha);
    }
    return input.color;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotbar_layout_hit_tests_nine_slots_without_gaps() {
        let layout = UiLayout::new(1280, 720, 1.0, UiScreen::Playing);
        for index in 0..9 {
            let rect = layout.rect(UiControl::HotbarSlot(index)).unwrap();
            assert_eq!(
                layout.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(UiControl::HotbarSlot(index))
            );
        }
        assert!(layout.rect(UiControl::HotbarSlot(9)).is_none());
    }

    #[test]
    fn menu_layouts_expose_only_visible_actions() {
        let pause = UiLayout::new(640, 360, 1.0, UiScreen::Pause);
        assert!(pause.rect(UiControl::Resume).is_some());
        assert!(pause.rect(UiControl::OpenSettings).is_some());
        assert!(pause.rect(UiControl::Exit).is_some());
        assert!(pause.rect(UiControl::ToggleFullscreen).is_none());

        let inventory = UiLayout::new(1280, 720, 1.0, UiScreen::Inventory);
        for block in [1, 2, 3] {
            assert!(inventory.rect(UiControl::CatalogBlock(block)).is_some());
        }
    }

    #[test]
    fn settings_layout_has_adjusters_and_fullscreen_toggle() {
        let layout = UiLayout::new(1280, 720, 1.0, UiScreen::Settings);
        for setting in [
            SettingId::Sensitivity,
            SettingId::FieldOfView,
            SettingId::ViewDistance,
            SettingId::UiScale,
        ] {
            assert!(layout.rect(UiControl::Decrease(setting)).is_some());
            assert!(layout.rect(UiControl::Increase(setting)).is_some());
        }
        assert!(layout.rect(UiControl::ToggleFullscreen).is_some());
        assert!(layout.rect(UiControl::Back).is_some());
    }

    #[test]
    fn compact_controls_hit_test_at_their_visible_centers() {
        let inventory = UiLayout::new(640, 360, 1.0, UiScreen::Inventory);
        let card = inventory.rect(UiControl::CatalogBlock(2)).unwrap();
        assert_eq!(
            inventory.hit_test(card.x + card.width * 0.5, card.y + card.height * 0.5),
            Some(UiControl::CatalogBlock(2))
        );

        let settings = UiLayout::new(640, 360, 1.0, UiScreen::Settings);
        for control in [
            UiControl::Decrease(SettingId::Sensitivity),
            UiControl::Increase(SettingId::ViewDistance),
            UiControl::ToggleFullscreen,
            UiControl::Back,
        ] {
            let rect = settings.rect(control).unwrap();
            assert_eq!(
                settings.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(control)
            );
        }

        let pause = UiLayout::new(640, 360, 1.0, UiScreen::Pause);
        for control in [UiControl::Resume, UiControl::OpenSettings, UiControl::Exit] {
            let rect = pause.rect(control).unwrap();
            assert_eq!(
                pause.hit_test(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
                Some(control)
            );
        }
    }

    #[test]
    fn worst_case_ui_stays_well_within_fixed_vertex_budget() {
        let long_status =
            "CONNECTION MESSAGE THAT SHOULD BE CLIPPED BEFORE IT CAN GROW THE UI BUFFER";
        let debug = UiDebug {
            position: [1234.5, -12.0, 9876.25],
            fps: 60.0,
            frame_ms: 16.6,
            visible_chunks: 512,
            cached_chunks: 512,
            latency_ms: Some(250),
        };
        for (width, height, scale) in [(1280, 720, 2.0), (640, 360, 2.0)] {
            for screen in [
                UiScreen::Playing,
                UiScreen::Inventory,
                UiScreen::Pause,
                UiScreen::Settings,
            ] {
                let frame = UiFrame {
                    screen,
                    selected_slot: 8,
                    hotbar: [1, 2, 3, 1, 2, 3, 1, 2, 3],
                    target: Some([10, 20, -30]),
                    status: Some(long_status),
                    debug: Some(debug),
                    catalog_selection: 3,
                    settings: UiSettings {
                        scale,
                        ..UiSettings::default()
                    },
                    hovered: Some(UiControl::Increase(SettingId::FieldOfView)),
                };
                let layout = UiLayout::new(width, height, scale, screen);
                let mut vertices = Vec::with_capacity(MAX_UI_VERTICES);
                let mut builder = UiBuilder {
                    vertices: &mut vertices,
                    width: width as f32,
                    height: height as f32,
                    scale,
                };
                builder.draw_frame(&frame, &layout);
                assert!(
                    vertices.len() < MAX_UI_VERTICES / 2,
                    "{screen:?} at {width}x{height}: {} vertices",
                    vertices.len()
                );
            }
        }
    }
}
