//! Temporary egui overlay for evaluating a shared game UI foundation.
//! Gameplay requests still go through the client and authoritative server.

use crate::content::Catalog;
use crate::ui::UiFrame;
use winit::{event::WindowEvent, window::Window};

mod view;
pub(crate) use view::{draw, themed_context};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intent {
    InventorySlot(u8, bool),
    ContainerSlot(u8, bool),
    Close,
}

pub(super) struct Proof {
    context: egui::Context,
    input: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    open: bool,
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

impl Proof {
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
            open: false,
            search: String::new(),
            filter: SlotFilter::All,
            intents: Vec::new(),
        }
    }

    pub(super) fn set_open(&mut self, open: bool) {
        self.open = open;
        self.intents.clear();
    }

    pub(super) fn on_window_event(&mut self, window: &Window, event: &WindowEvent) {
        if self.open {
            let _ = self.input.on_window_event(window, event);
        }
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
        if !self.open {
            return;
        }
        let raw_input = self.input.take_egui_input(target.window);
        let mut output = self.context.run_ui(raw_input, |ui| {
            draw(
                ui,
                frame,
                catalog,
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
                    label: Some("egui proof overlay"),
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
