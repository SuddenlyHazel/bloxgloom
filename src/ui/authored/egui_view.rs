//! Egui presentation of verified, bounded package documents.

use super::*;
use egui::{Color32, FontFamily, FontId, RichText, Vec2};

#[derive(Debug)]
pub(crate) enum Intent {
    Guarded {
        generation: u64,
        intent: Box<Intent>,
    },
    Activate(usize),
    Input(usize, String),
    Focus(usize),
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
        let mut current = Vec::new();
        self.draw_egui_inner(ui, atlas, &mut current);
        intents.extend(current.into_iter().map(|intent| Intent::Guarded {
            generation: self.tree_generation,
            intent: Box::new(intent),
        }));
    }

    fn draw_egui_inner(
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
        ui.push_id(&node.id, |ui| match node.kind {
            Kind::Panel | Kind::ScrollPanel | Kind::Table => {
                egui::Frame::new()
                    .fill(rgba(node.style.background))
                    .inner_margin(egui::Margin::same(node.style.padding as i8))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        if node.kind == Kind::ScrollPanel {
                            egui::ScrollArea::both()
                                .id_salt("scroll-panel")
                                .max_height(height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.draw_children(ui, index, atlas, intents));
                        } else if node.kind == Kind::Table {
                            let columns = self
                                .children(index)
                                .into_iter()
                                .map(|child| {
                                    if self.document().nodes[child].kind.container() {
                                        self.children(child).len()
                                    } else {
                                        1
                                    }
                                })
                                .max()
                                .unwrap_or(1)
                                .max(1);
                            let gap = f32::from(node.style.gap);
                            let cell_width =
                                ((width - gap * (columns - 1) as f32) / columns as f32).max(1.0);
                            egui::Grid::new("table")
                                .num_columns(columns)
                                .min_col_width(cell_width)
                                .max_col_width(cell_width)
                                .striped(true)
                                .spacing([f32::from(node.style.gap); 2])
                                .show(ui, |ui| {
                                    for child in self.children(index) {
                                        if !self.is_visible(child) {
                                            continue;
                                        }
                                        if self.document().nodes[child].kind.container() {
                                            for cell in self.children(child) {
                                                self.draw_node(ui, cell, atlas, intents);
                                            }
                                        } else {
                                            self.draw_node(ui, child, atlas, intents);
                                        }
                                        ui.end_row();
                                    }
                                });
                        } else {
                            ui.set_min_height(height);
                            self.draw_children(ui, index, atlas, intents);
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
            Kind::Input | Kind::MultilineInput => {
                let mut value = self.inputs[index].clone();
                let edit = if node.kind == Kind::MultilineInput {
                    egui::TextEdit::multiline(&mut value)
                } else {
                    egui::TextEdit::singleline(&mut value)
                };
                let response = ui.add_sized(
                    [width, height],
                    edit.id(egui::Id::new(&node.id))
                        .font(font)
                        .char_limit(super::controls::Control::limit(node.kind)),
                );
                if response.has_focus() {
                    intents.push(Intent::Focus(index));
                }
                if response.changed() {
                    trim_bytes(&mut value, super::controls::Control::limit(node.kind));
                    intents.push(Intent::Input(index, value));
                }
            }
            Kind::Checkbox => {
                let mut checked = self.inputs[index] == "true";
                let response = ui.add_sized(
                    [width, height],
                    egui::Checkbox::new(
                        &mut checked,
                        RichText::new(self.text_at(index)).font(font).color(color),
                    ),
                );
                if response.has_focus() {
                    intents.push(Intent::Focus(index));
                }
                if response.changed() {
                    intents.push(Intent::Input(index, checked.to_string()));
                }
            }
            Kind::Slider => {
                if let super::controls::Control::Slider {
                    value: initial,
                    min,
                    max,
                    step,
                } = node.control
                {
                    let mut value = self.inputs[index].parse::<f64>().unwrap_or(initial);
                    let mut slider = egui::Slider::new(&mut value, min..=max)
                        .text(RichText::new(self.text_at(index)).font(font).color(color));
                    if let Some(step) = step {
                        slider = slider.step_by(step);
                    }
                    let response = ui.add_sized([width, height], slider);
                    if response.has_focus() {
                        intents.push(Intent::Focus(index));
                    }
                    if response.changed() {
                        intents.push(Intent::Input(
                            index,
                            super::controls::Control::number(value),
                        ));
                    }
                }
            }
            Kind::Select => {
                if let super::controls::Control::Select { options, .. } = &node.control {
                    ui.label(RichText::new(self.text_at(index)).font(font).color(color));
                    let mut selected = self.inputs[index].clone();
                    let response = egui::ComboBox::from_id_salt("select")
                        .width(width)
                        .selected_text(node.control.display(&selected))
                        .show_ui(ui, |ui| {
                            for option in options {
                                ui.selectable_value(
                                    &mut selected,
                                    option.key.clone(),
                                    &option.label,
                                );
                            }
                        });
                    if response.response.has_focus() {
                        intents.push(Intent::Focus(index));
                    }
                    if selected != self.inputs[index] {
                        intents.push(Intent::Input(index, selected));
                    }
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
        });
    }

    fn children(&self, index: usize) -> Vec<usize> {
        self.document()
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(child, widget)| (widget.parent == Some(index)).then_some(child))
            .collect()
    }
    fn draw_children(
        &self,
        ui: &mut egui::Ui,
        index: usize,
        atlas: Option<egui::TextureId>,
        intents: &mut Vec<Intent>,
    ) {
        let node = &self.document().nodes[index];
        let children = self.children(index);
        if node.style.row {
            ui.horizontal_wrapped(|ui| {
                for child in children {
                    self.draw_node(ui, child, atlas, intents);
                    ui.add_space(f32::from(node.style.gap));
                }
            });
        } else {
            for child in children {
                self.draw_node(ui, child, atlas, intents);
                ui.add_space(f32::from(node.style.gap));
            }
        }
    }

    pub(crate) fn apply_egui(&mut self, intent: Intent) {
        match intent {
            Intent::Guarded { generation, intent } if generation == self.tree_generation => {
                self.apply_egui(*intent)
            }
            Intent::Focus(index)
                if self
                    .document()
                    .nodes
                    .get(index)
                    .is_some_and(|n| n.kind.input())
                    && self.is_visible(index) =>
            {
                self.focused = Some(index)
            }
            Intent::Activate(index) if self.egui_focusable(index, Kind::Button) => {
                self.focused = Some(index);
                self.activate();
            }
            Intent::Input(index, value)
                if self
                    .document()
                    .nodes
                    .get(index)
                    .is_some_and(|node| node.kind.input())
                    && self.is_visible(index) =>
            {
                self.focused = Some(index);
                let node = &self.document().nodes[index];
                if !node.control.validate(node.kind, &value) || !self.can_dispatch(index) {
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

fn trim_bytes(value: &mut String, limit: usize) {
    if value.len() > limit {
        let mut end = limit;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
}
