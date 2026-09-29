//! Egui presentation of verified, bounded package documents.

use super::*;
use egui::{Color32, FontFamily, FontId, RichText, Vec2};

#[derive(Debug)]
pub(crate) enum Intent {
    Activate(usize),
    Input(usize, String),
    NextDocument,
}

impl Resources {
    pub(crate) fn install_egui(&self, context: &egui::Context) -> egui::TextureHandle {
        let mut definitions = egui::FontDefinitions::default();
        let fallback = definitions.families[&FontFamily::Proportional].clone();
        for (id, bytes) in &self.font_sources {
            definitions.font_data.insert(
                id.clone(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes.clone())),
            );
            let mut family = vec![id.clone()];
            family.extend(fallback.iter().cloned());
            definitions
                .families
                .insert(FontFamily::Name(id.clone().into()), family);
        }
        context.set_fonts(definitions);
        let atlas =
            egui::ColorImage::from_rgba_unmultiplied([ATLAS_SIZE, ATLAS_SIZE], &self.pixels);
        context.load_texture(
            "verified-package-ui-atlas",
            atlas,
            egui::TextureOptions::NEAREST,
        )
    }
}

impl Session {
    pub(crate) fn draw_egui(
        &self,
        ui: &mut egui::Ui,
        atlas: Option<egui::TextureId>,
        intents: &mut Vec<Intent>,
    ) {
        let viewport = ui.max_rect();
        ui.painter()
            .rect_filled(viewport, 0.0, Color32::from_black_alpha(185));
        let compact = viewport.width() < 800.0 || viewport.height() < 500.0;
        let card = egui::Rect::from_center_size(
            viewport.center(),
            Vec2::new(
                (viewport.width() - if compact { 12.0 } else { 70.0 }).min(900.0),
                (viewport.height() - if compact { 12.0 } else { 70.0 }).min(650.0),
            ),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(33, 45, 42))
                .stroke(egui::Stroke::new(2.0, Color32::from_rgb(92, 111, 98)))
                .corner_radius(egui::CornerRadius::same(11))
                .inner_margin(egui::Margin::same(if compact { 10 } else { 20 }))
                .show(ui, |ui| {
                    ui.set_width(card.width() - if compact { 20.0 } else { 40.0 });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("PACKAGE  /  {}", self.document().id))
                                .monospace()
                                .color(Color32::from_rgb(224, 191, 111)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("NEXT  PgDn").clicked() {
                                intents.push(Intent::NextDocument);
                            }
                        });
                    });
                    if let Some(feedback) = &self.feedback {
                        ui.label(RichText::new(feedback).color(Color32::from_rgb(224, 191, 111)));
                    }
                    ui.separator();
                    egui::ScrollArea::both()
                        .id_salt(("package-document", &self.document().id))
                        .max_height(card.height() - if compact { 115.0 } else { 100.0 })
                        .show(ui, |ui| self.draw_node(ui, 0, atlas, intents));
                    ui.separator();
                    ui.label(
                        RichText::new("Escape closes  /  Tab changes focus")
                            .size(11.0)
                            .color(Color32::from_rgb(167, 181, 166)),
                    );
                });
        });
    }

    fn draw_node(
        &self,
        ui: &mut egui::Ui,
        index: usize,
        atlas: Option<egui::TextureId>,
        intents: &mut Vec<Intent>,
    ) {
        if !self.is_visible(index) {
            return;
        }
        let node = &self.document().nodes[index];
        let width = if node.style.width == 0 {
            ui.available_width()
        } else {
            f32::from(node.style.width).min(ui.available_width())
        };
        let height = f32::from(node.style.height);
        let color = rgba(node.style.color);
        let font = FontId::new(
            18.0,
            node.style
                .font
                .as_ref()
                .map_or(FontFamily::Proportional, |id| {
                    FontFamily::Name(id.clone().into())
                }),
        );
        match node.kind {
            Kind::Panel => {
                let background = rgba(node.style.background);
                let children = self
                    .document()
                    .nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(child, widget)| (widget.parent == Some(index)).then_some(child))
                    .collect::<Vec<_>>();
                egui::Frame::new()
                    .fill(background)
                    .inner_margin(egui::Margin::same(node.style.padding as i8))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.set_min_height(height);
                        if node.style.row {
                            ui.horizontal_wrapped(|ui| {
                                for child in &children {
                                    self.draw_node(ui, *child, atlas, intents);
                                    ui.add_space(f32::from(node.style.gap));
                                }
                            });
                        } else {
                            for child in children {
                                self.draw_node(ui, child, atlas, intents);
                                ui.add_space(f32::from(node.style.gap));
                            }
                        }
                    });
            }
            Kind::Label => {
                ui.add_sized(
                    [width, height],
                    egui::Label::new(RichText::new(self.text_at(index)).font(font).color(color))
                        .wrap(),
                );
            }
            Kind::Button => {
                if ui
                    .add_sized(
                        [width, height],
                        egui::Button::new(
                            RichText::new(self.text_at(index)).font(font).color(color),
                        ),
                    )
                    .clicked()
                {
                    intents.push(Intent::Activate(index));
                }
            }
            Kind::Input => {
                let mut value = self.text_at(index).to_owned();
                let response = ui.add_sized(
                    [width, height],
                    egui::TextEdit::singleline(&mut value)
                        .font(font)
                        .char_limit(MAX_TEXT),
                );
                if response.changed() {
                    intents.push(Intent::Input(index, value));
                }
            }
            Kind::Image => {
                if let (Some(texture), Some(key)) = (atlas, &node.image) {
                    let image = self.resources.images[key];
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(image.x / ATLAS_SIZE as f32, image.y / ATLAS_SIZE as f32),
                        egui::pos2(
                            (image.x + image.width) / ATLAS_SIZE as f32,
                            (image.y + image.height) / ATLAS_SIZE as f32,
                        ),
                    );
                    ui.add(egui::Image::new((texture, Vec2::new(width, height))).uv(uv));
                }
            }
        }
    }

    pub(crate) fn apply_egui(&mut self, intent: Intent) {
        match intent {
            Intent::Activate(index) if self.egui_focusable(index, Kind::Button) => {
                self.focused = Some(index);
                self.activate();
            }
            Intent::Input(index, value) if self.egui_focusable(index, Kind::Input) => {
                self.focused = Some(index);
                if value.len() > MAX_TEXT
                    || value.chars().any(char::is_control)
                    || !self.can_dispatch(index)
                {
                    return;
                }
                let old = std::mem::replace(&mut self.inputs[index], value);
                if self.inputs[index] != old && !self.dispatch(index) {
                    self.inputs[index] = old;
                }
            }
            Intent::NextDocument => self.next_document(),
            _ => {}
        }
    }

    fn egui_focusable(&self, index: usize, kind: Kind) -> bool {
        self.document()
            .nodes
            .get(index)
            .is_some_and(|node| node.kind == kind)
            && self.is_visible(index)
    }
}

fn rgba(value: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(value[0], value[1], value[2], value[3])
}
