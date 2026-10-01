//! Live egui UI for built-in screens and verified package documents.
//! Gameplay requests still go through the client and authoritative server.

use crate::content::Catalog;
use crate::ui::{UiControl, UiFrame, UiScreen};
use winit::{event::WindowEvent, window::Window};

mod hud;
mod menus;
mod view;
pub(crate) use view::{draw, themed_context};

pub(crate) fn draw_screen(
    ui: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    package_atlas: Option<egui::TextureId>,
    search: &mut String,
    filter: &mut SlotFilter,
    intents: &mut Vec<Intent>,
) {
    if frame.screen == UiScreen::Package {
        if let Some(session) = frame.package_ui {
            let mut authored = Vec::new();
            session.draw_egui(ui, package_atlas, &mut authored);
            intents.extend(authored.into_iter().map(Intent::Package));
        }
    } else if frame.screen == UiScreen::Playing {
        hud::draw(ui, frame, catalog);
    } else if matches!(frame.screen, UiScreen::Inventory | UiScreen::Container) {
        draw(ui, frame, catalog, search, filter, intents);
    } else {
        menus::draw(ui, frame, catalog, intents);
    }
}

#[derive(Debug)]
pub(crate) enum Intent {
    InventorySlot(u8, bool),
    ContainerSlot(u8, bool),
    Close,
    Control(UiControl),
    AdminInput(String),
    Package(crate::ui::authored::EguiIntent),
    JoinAddress(String),
    JoinAction,
}

pub(super) struct GameUi {
    context: egui::Context,
    input: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    package_atlas: Option<egui::TextureHandle>,
    search: String,
    filter: SlotFilter,
    intents: Vec<Intent>,
}

pub(super) struct DrawTarget<'a> {
    pub window: &'a Window,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub size: [u32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotFilter {
    All,
    Hotbar,
    Backpack,
}

impl SlotFilter {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All slots",
            Self::Hotbar => "Hotbar",
            Self::Backpack => "Backpack",
        }
    }
}

impl GameUi {
    pub(super) fn new(window: &Window, device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let context = themed_context();
        let input = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(device, format, Default::default());
        Self {
            context,
            input,
            renderer,
            package_atlas: None,
            search: String::new(),
            filter: SlotFilter::All,
            intents: Vec::new(),
        }
    }

    pub(super) fn clear_intents(&mut self) {
        self.intents.clear();
    }

    pub(super) fn install_package_ui(&mut self, resources: &crate::ui::authored::Resources) {
        self.package_atlas = Some(resources.install_egui(&self.context));
    }

    pub(super) fn on_window_event(&mut self, window: &Window, event: &WindowEvent) {
        let _ = self.input.on_window_event(window, event);
    }

    pub(super) fn take_intents(&mut self) -> Vec<Intent> {
        std::mem::take(&mut self.intents)
    }

    pub(super) fn wants_keyboard_input(&self) -> bool {
        self.context.egui_wants_keyboard_input()
    }

    pub(super) fn encode(
        &mut self,
        target: DrawTarget<'_>,
        frame: &UiFrame<'_>,
        catalog: &Catalog,
    ) {
        self.context
            .set_zoom_factor(frame.settings.scale.clamp(0.75, 2.0));
        let raw_input = self.input.take_egui_input(target.window);
        let mut output = self.context.run_ui(raw_input, |ui| {
            draw_screen(
                ui,
                frame,
                catalog,
                self.package_atlas.as_ref().map(egui::TextureHandle::id),
                &mut self.search,
                &mut self.filter,
                &mut self.intents,
            );
        });
        self.input
            .handle_platform_output(target.window, output.platform_output);
        for (id, deltas) in output.textures_delta.set.drain() {
            for delta in &deltas {
                self.renderer
                    .update_texture(target.device, target.queue, id, delta);
            }
        }
        let paint = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: target.size,
            pixels_per_point: output.pixels_per_point,
        };
        self.renderer
            .update_buffers(target.device, target.queue, target.encoder, &paint, &screen);
        {
            let pass = target
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("game ui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target.view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    ..Default::default()
                });
            self.renderer
                .render(&mut pass.forget_lifetime(), &paint, &screen);
        }
        for id in output.textures_delta.free.drain() {
            self.renderer.free_texture(&id);
        }
    }
}
