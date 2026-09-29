//! A compact game overlay built from egui controls and custom slot painting.

mod slot;

use super::{Intent, SlotFilter};
use crate::{content::Catalog, inventory::Stack, ui::UiFrame};
use bloxgloom_host_api::StatusFormat;
use egui::{Align, Align2, Color32, FontId, Margin, RichText, Stroke, TextStyle, Vec2};
use slot::SlotStyle;

const PANEL: Color32 = Color32::from_rgb(33, 45, 42);
const PANEL_EDGE: Color32 = Color32::from_rgb(92, 111, 98);
const SLOT: Color32 = Color32::from_rgb(49, 62, 58);
const SLOT_HOVER: Color32 = Color32::from_rgb(66, 82, 73);
const TEXT: Color32 = Color32::from_rgb(236, 239, 221);
const MUTED: Color32 = Color32::from_rgb(167, 181, 166);
const GOLD: Color32 = Color32::from_rgb(224, 191, 111);

pub(crate) fn themed_context() -> egui::Context {
    let context = egui::Context::default();
    let mut style = (*context.global_style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = Color32::from_rgb(22, 32, 30);
    style.visuals.selection.bg_fill = Color32::from_rgb(115, 98, 55);
    style.visuals.widgets.inactive.bg_fill = SLOT;
    style.visuals.widgets.hovered.bg_fill = SLOT_HOVER;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(82, 91, 62);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, PANEL_EDGE);
    style.spacing.item_spacing = Vec2::new(6.0, 6.0);
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(24.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::monospace(11.0));
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
    let viewport = root.max_rect();
    let compact = viewport.width() < 800.0 || viewport.height() < 500.0;
    let margin = if compact { 8.0 } else { 40.0 };
    let panel_width = (viewport.width() - margin * 2.0).min(if compact { 660.0 } else { 940.0 });
    let padding = if compact { 12 } else { 22 };
    let content_width = panel_width - f32::from(padding) * 2.0;
    let sidebar_width = if compact { 175.0 } else { 225.0 };
    let gap = if compact { 12.0 } else { 24.0 };
    let inventory_width = content_width - sidebar_width - gap;
    let content_height = if compact { 226.0 } else { 354.0 };

    root.painter()
        .rect_filled(viewport, 0.0, Color32::from_black_alpha(160));
    let card_height: f32 = if compact { 344.0 } else { 570.0 };
    let card = egui::Rect::from_center_size(
        viewport.center(),
        Vec2::new(panel_width, card_height.min(viewport.height() - 8.0)),
    );
    root.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(2.0, PANEL_EDGE))
            .corner_radius(egui::CornerRadius::same(11))
            .inner_margin(Margin::same(padding))
            .show(ui, |ui| {
                ui.set_width(content_width);
                header(ui, frame, intents, compact);
                ui.add_space(if compact { 8.0 } else { 16.0 });
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(inventory_width, content_height),
                        egui::Layout::top_down(Align::LEFT),
                        |ui| {
                            inventory(ui, frame, catalog, search, filter, intents, compact);
                        },
                    );
                    ui.add_space(gap);
                    ui.allocate_ui_with_layout(
                        Vec2::new(sidebar_width, content_height),
                        egui::Layout::top_down(Align::LEFT),
                        |ui| machine(ui, frame, catalog, intents, compact),
                    );
                });
                ui.add_space(if compact { 3.0 } else { 9.0 });
                footer(ui, frame, catalog, compact);
            });
    });
}

fn header(ui: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>, compact: bool) {
    let title = frame
        .container_screen
        .as_ref()
        .map_or("Field pack", |screen| screen.title.as_str());
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new("BLOXGLOOM  /  STORAGE")
                    .monospace()
                    .size(if compact { 9.0 } else { 11.0 })
                    .color(GOLD),
            );
            ui.label(
                RichText::new(title.to_uppercase())
                    .strong()
                    .size(if compact { 20.0 } else { 28.0 })
                    .color(TEXT),
            );
        });
        ui.add_space((ui.available_width() - if compact { 64.0 } else { 81.0 }).max(0.0));
        let close = ui.add_sized(
            [
                if compact { 58.0 } else { 75.0 },
                if compact { 25.0 } else { 31.0 },
            ],
            egui::Button::new("CLOSE  F7"),
        );
        if close.clicked() {
            intents.push(Intent::Close);
        }
    });
    ui.separator();
}

fn inventory(
    ui: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    search: &mut String,
    filter: &mut SlotFilter,
    intents: &mut Vec<Intent>,
    compact: bool,
) {
    let width = ui.available_width();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("INVENTORY")
                .strong()
                .size(if compact { 12.0 } else { 15.0 }),
        );
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new("36 SLOTS")
                    .monospace()
                    .size(10.0)
                    .color(MUTED),
            );
        });
    });
    ui.add_space(if compact { 2.0 } else { 6.0 });
    ui.horizontal(|ui| {
        let search_width = (width - if compact { 123.0 } else { 153.0 }).max(80.0);
        ui.add_sized(
            [search_width, if compact { 25.0 } else { 31.0 }],
            egui::TextEdit::singleline(search).hint_text("Search items…"),
        );
        egui::ComboBox::from_id_salt("egui-proof-filter")
            .width(if compact { 110.0 } else { 137.0 })
            .selected_text(filter.label())
            .show_ui(ui, |ui| {
                for choice in [SlotFilter::All, SlotFilter::Hotbar, SlotFilter::Backpack] {
                    ui.selectable_value(filter, choice, choice.label());
                }
            });
    });
    ui.add_space(if compact { 6.0 } else { 12.0 });
    let gap = if compact { 4.0 } else { 6.0 };
    let side = ((width - gap * 8.0) / 9.0).min(if compact { 34.0 } else { 62.0 });
    let starts: &[u8] = match filter {
        SlotFilter::All => &[9, 18, 27, 0],
        SlotFilter::Hotbar => &[0],
        SlotFilter::Backpack => &[9, 18, 27],
    };
    let query = search.trim().to_lowercase();
    for (row, &first) in starts.iter().enumerate() {
        if row > 0 {
            ui.add_space(if first == 0 { gap + 4.0 } else { gap });
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for slot_number in first..first + 9 {
                let stack = frame.inventory[usize::from(slot_number)].as_ref();
                let matches = !query.is_empty()
                    && stack.is_some_and(|stack| {
                        catalog
                            .item(stack.item)
                            .is_some_and(|item| item.name.to_lowercase().contains(&query))
                    });
                let response = slot::show(
                    ui,
                    slot_number,
                    stack,
                    catalog,
                    SlotStyle {
                        side,
                        selected: frame.inventory_source == Some(slot_number),
                        highlighted: matches,
                        dimmed: !query.is_empty() && stack.is_some() && !matches,
                        hotbar: slot_number < 9,
                    },
                );
                if response.clicked() || response.secondary_clicked() {
                    intents.push(Intent::InventorySlot(
                        slot_number,
                        response.secondary_clicked(),
                    ));
                }
            }
        });
    }
    if !compact {
        ui.add_space(10.0);
        ui.label(
            RichText::new("SELECT A SOURCE, THEN A DESTINATION")
                .monospace()
                .size(10.0)
                .color(MUTED),
        );
    }
}

fn machine(
    ui: &mut egui::Ui,
    frame: &UiFrame<'_>,
    catalog: &Catalog,
    intents: &mut Vec<Intent>,
    compact: bool,
) {
    let width = ui.available_width();
    egui::Frame::new()
        .fill(Color32::from_rgb(27, 38, 35))
        .stroke(Stroke::new(1.0, PANEL_EDGE))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(Margin::same(if compact { 10 } else { 14 }))
        .show(ui, |ui| {
            ui.set_min_width(width - if compact { 20.0 } else { 28.0 });
            if let (Some(screen), Some(view)) = (&frame.container_screen, &frame.kiln) {
                ui.label(
                    RichText::new("WORKSTATION")
                        .monospace()
                        .size(10.0)
                        .color(GOLD),
                );
                ui.add_space(if compact { 3.0 } else { 9.0 });
                for (field, value) in screen.status.iter().zip(&view.status) {
                    status(ui, field, *value, compact);
                }
                if !screen.status.is_empty() {
                    ui.add_space(if compact { 5.0 } else { 12.0 });
                    ui.separator();
                }
                egui::ScrollArea::vertical()
                    .id_salt("egui-proof-machine-slots")
                    .max_height(if compact { 128.0 } else { 220.0 })
                    .show(ui, |ui| {
                        let columns = usize::from(screen.columns);
                        let gap = if compact { 4.0 } else { 7.0 };
                        let side = ((ui.available_width()
                            - gap * (columns.saturating_sub(1)) as f32)
                            / columns as f32)
                            .min(if compact { 45.0 } else { 62.0 });
                        for row in view.slots.chunks(columns).enumerate() {
                            let (row_number, stacks) = row;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for (col, stack) in stacks.iter().enumerate() {
                                    let number = (row_number * columns + col) as u8;
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        if columns <= 3 {
                                            let label = screen
                                                .group(number)
                                                .map_or("SLOT", |group| group.label.as_str());
                                            ui.label(
                                                RichText::new(label)
                                                    .monospace()
                                                    .size(if compact { 8.0 } else { 9.0 })
                                                    .color(MUTED),
                                            );
                                        }
                                        let response = slot::show(
                                            ui,
                                            number,
                                            stack.as_ref(),
                                            catalog,
                                            SlotStyle {
                                                side,
                                                selected: frame.kiln_source == Some(number),
                                                highlighted: false,
                                                dimmed: false,
                                                hotbar: false,
                                            },
                                        );
                                        if response.clicked() || response.secondary_clicked() {
                                            intents.push(Intent::ContainerSlot(
                                                number,
                                                response.secondary_clicked(),
                                            ));
                                        }
                                    });
                                }
                            });
                            ui.add_space(gap);
                        }
                    });
                if !compact && !screen.hint.is_empty() {
                    ui.add_space(7.0);
                    ui.label(RichText::new(&screen.hint).size(11.0).color(MUTED));
                }
            } else {
                ui.label(
                    RichText::new("FIELD NOTES")
                        .monospace()
                        .size(10.0)
                        .color(GOLD),
                );
                ui.add_space(10.0);
                ui.label(RichText::new("Your pack, at a glance.").size(if compact {
                    13.0
                } else {
                    16.0
                }));
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "Open a kiln, press, or chest to transfer items with the server.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
            }
        });
}

fn status(ui: &mut egui::Ui, field: &bloxgloom_host_api::StatusField, value: u32, compact: bool) {
    let display = match field.format {
        StatusFormat::Number => format!("{value}"),
        StatusFormat::Milliseconds => format!("{:.1} s", value as f32 / 1000.0),
        StatusFormat::Progress => format!("{}%", u64::from(value) * 100 / u64::from(field.maximum)),
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(field.label.to_uppercase())
                .monospace()
                .size(if compact { 9.0 } else { 10.0 })
                .color(MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(display)
                    .monospace()
                    .size(if compact { 10.0 } else { 12.0 })
                    .color(TEXT),
            );
        });
    });
    if field.format == StatusFormat::Progress {
        ui.add(
            egui::ProgressBar::new((value as f32 / field.maximum as f32).clamp(0.0, 1.0))
                .desired_width(ui.available_width())
                .fill(GOLD),
        );
    }
    ui.add_space(if compact { 2.0 } else { 6.0 });
}

fn footer(ui: &mut egui::Ui, frame: &UiFrame<'_>, catalog: &Catalog, compact: bool) {
    ui.separator();
    let source = frame
        .kiln_source
        .and_then(|slot| frame.kiln.as_ref()?.slots.get(usize::from(slot))?.as_ref())
        .or_else(|| {
            frame
                .inventory_source
                .and_then(|slot| frame.inventory[usize::from(slot)].as_ref())
        });
    let selected = source
        .and_then(|stack: &Stack| catalog.item(stack.item))
        .map_or("CHOOSE A SOURCE".to_owned(), |item| {
            format!("SOURCE  /  {}", item.name)
        });
    ui.horizontal(|ui| {
        let message = frame.status.unwrap_or(&selected);
        let message_width = (ui.available_width() - if compact { 130.0 } else { 210.0 }).max(70.0);
        let (message_rect, response) =
            ui.allocate_exact_size(Vec2::new(message_width, 16.0), egui::Sense::hover());
        let max_chars = if compact { 42 } else { 74 };
        let shown = if message.chars().count() > max_chars {
            format!("{}…", message.chars().take(max_chars).collect::<String>())
        } else {
            message.to_owned()
        };
        ui.painter().text(
            message_rect.left_center(),
            Align2::LEFT_CENTER,
            shown,
            FontId::monospace(if compact { 9.0 } else { 11.0 }),
            GOLD,
        );
        response.on_hover_text(message);
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(if frame.container_screen.is_some() {
                    "LEFT: STACK    RIGHT: ONE"
                } else {
                    "LEFT: STACK    RIGHT: HALF"
                })
                .monospace()
                .size(if compact { 8.0 } else { 10.0 })
                .color(MUTED),
            );
        });
    });
}
