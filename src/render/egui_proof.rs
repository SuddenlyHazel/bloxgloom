//! Temporary egui overlay for evaluating a shared game UI foundation.
//! Gameplay requests still go through the client and authoritative server.

use crate::content::Catalog;
use crate::ui::UiFrame;
use winit::{event::WindowEvent, window::Window};

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
    fn label(self) -> &'static str {
        match self {
            Self::All => "All slots",
            Self::Hotbar => "Hotbar",
            Self::Backpack => "Backpack",
        }
    }

    fn includes(self, slot: u8) -> bool {
        match self {
            Self::All => true,
            Self::Hotbar => slot < 9,
            Self::Backpack => slot >= 9,
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

pub(crate) fn themed_context() -> egui::Context {
    let context = egui::Context::default();
    let mut style = (*context.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = egui::Color32::from_rgb(20, 31, 30);
    style.visuals.window_fill = egui::Color32::from_rgb(23, 36, 34);
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(142, 117, 48);
    context.set_global_style(style);
    context
}

pub(crate) fn draw(
    root: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    search: &mut String,
    filter: &mut SlotFilter,
    intents: &mut Vec<Intent>,
) {
    egui::CentralPanel::default().show(root, |ui| {
        ui.horizontal(|ui| {
            ui.heading("BLOXGLOOM  /  EGUI PROOF");
            ui.add_space(12.0);
            if ui.button("Close  F7").clicked() {
                intents.push(Intent::Close);
            }
        });
        ui.label("Live inventory and container state. Click a source slot, then a destination. Right-click moves one item into or out of a container.");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.add(egui::TextEdit::singleline(search).hint_text("Item name"));
            egui::ComboBox::from_id_salt("egui-proof-filter")
                .selected_text(filter.label())
                .show_ui(ui, |ui| {
                    for choice in [SlotFilter::All, SlotFilter::Hotbar, SlotFilter::Backpack] {
                        ui.selectable_value(filter, choice, choice.label());
                    }
                });
        });
        if let Some(status) = frame.status {
            ui.colored_label(egui::Color32::from_rgb(226, 190, 98), status);
        }
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            let inventory = &mut columns[0];
            inventory.heading("PLAYER INVENTORY");
            egui::ScrollArea::vertical()
                .id_salt("egui-proof-inventory")
                .auto_shrink([false, false])
                .show(inventory, |ui| {
                    for slot in 0..crate::inventory::SLOTS as u8 {
                        if !filter.includes(slot) {
                            continue;
                        }
                        let stack = frame.inventory[slot as usize].as_ref();
                        if !matches_search(stack, search, catalog) {
                            continue;
                        }
                        let label = slot_label(slot, stack, catalog);
                        let selected = frame.inventory_source == Some(slot);
                        let response = ui.add_sized(
                            [ui.available_width().min(330.0), 30.0],
                            egui::Button::new(label).selected(selected),
                        );
                        if response.clicked() || response.secondary_clicked() {
                            intents.push(Intent::InventorySlot(slot, response.secondary_clicked()));
                        }
                    }
                });
            let container = &mut columns[1];
            if let (Some(screen), Some(view)) = (&frame.container_screen, &frame.kiln) {
                container.heading(&screen.title);
                for (field, value) in screen.status.iter().zip(&view.status) {
                    container.label(format!("{}: {}", field.label, value));
                }
                egui::ScrollArea::vertical()
                    .id_salt("egui-proof-container")
                    .auto_shrink([false, false])
                    .show(container, |ui| {
                        for (slot, stack) in view.slots.iter().enumerate() {
                            let label = slot_label(slot as u8, stack.as_ref(), catalog);
                            let selected = frame.kiln_source == Some(slot as u8);
                            let response = ui.add_sized(
                                [ui.available_width().min(330.0), 30.0],
                                egui::Button::new(label).selected(selected),
                            );
                            if response.clicked() || response.secondary_clicked() {
                                intents.push(Intent::ContainerSlot(
                                    slot as u8,
                                    response.secondary_clicked(),
                                ));
                            }
                        }
                    });
            } else {
                container.heading("FOUNDATION CHECK");
                container.label("This panel uses egui text input, a drop-down, scroll areas, buttons, focus and the existing wgpu surface.");
                container.label("Open a press or other container, then press F7 to try finite slot transfers through the normal server path.");
            }
        });
    });
}

fn matches_search(
    stack: Option<&crate::inventory::Stack>,
    search: &str,
    catalog: &Catalog,
) -> bool {
    search.is_empty()
        || stack.is_none()
        || stack.is_some_and(|stack| {
            catalog
                .item(stack.item)
                .is_some_and(|item| item.name.to_lowercase().contains(&search.to_lowercase()))
        })
}

fn slot_label(slot: u8, stack: Option<&crate::inventory::Stack>, catalog: &Catalog) -> String {
    match stack {
        Some(stack) => format!(
            "{:02}   {}  ×{}",
            slot + 1,
            catalog
                .item(stack.item)
                .map_or("Unknown", |item| item.name.as_ref()),
            stack.count
        ),
        None => format!("{:02}   Empty", slot + 1),
    }
}
