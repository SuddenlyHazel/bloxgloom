//! Compact bounded transcript, painted by the native gameplay HUD.
use crate::client::chat::Session;
pub(crate) fn draw(root: &mut egui::Ui, chat: &Session) {
    if chat.lines().is_empty() && !chat.open {
        return;
    }
    let viewport = root.max_rect();
    if viewport.width() < 100. || viewport.height() < 120. {
        return;
    }
    let width = (viewport.width() * 0.45)
        .clamp(230., 520.)
        .min(viewport.width() - 24.);
    let height = (if chat.open { 220.0_f32 } else { 140.0_f32 }).min(viewport.height() - 100.);
    let rect = egui::Rect::from_min_size(
        viewport.left_bottom() + egui::vec2(12., -height - 88.),
        egui::vec2(width, height),
    );
    root.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.set_clip_rect(rect);
        egui::Frame::new()
            .fill(egui::Color32::from_black_alpha(180))
            .corner_radius(5)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_min_width(width - 16.);
                ui.set_max_width(width - 16.);
                if chat.open {
                    egui::ScrollArea::vertical()
                        .id_salt("chat-transcript")
                        .stick_to_bottom(true)
                        .max_height((height - 65.).max(20.))
                        .show(ui, |ui| {
                            for line in chat.lines() {
                                ui.label(
                                    egui::RichText::new(line)
                                        .size(13.)
                                        .color(egui::Color32::WHITE),
                                );
                            }
                        });
                    ui.separator();
                    let color = egui::Color32::LIGHT_GREEN;
                    let galley = ui.painter().layout_no_wrap(
                        format!("> {}|", chat.input),
                        egui::FontId::proportional(14.0),
                        color,
                    );
                    let (input_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 20.0),
                        egui::Sense::hover(),
                    );
                    let overflow = (galley.size().x - input_rect.width()).max(0.0);
                    ui.painter().with_clip_rect(input_rect).galley(
                        input_rect.left_top() - egui::vec2(overflow, 0.0),
                        galley,
                        color,
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("Enter sends · Esc closes · Up/Down history")
                                .size(11.)
                                .color(egui::Color32::GRAY),
                        )
                        .truncate(),
                    );
                } else {
                    for line in chat
                        .lines()
                        .iter()
                        .rev()
                        .take(5)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                    {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(line)
                                    .size(13.)
                                    .color(egui::Color32::WHITE),
                            )
                            .truncate(),
                        );
                    }
                }
            });
    });
}
