//! Playing HUD painted by egui; all numbers come from streamed server state.

use super::view::slot::{self, SlotStyle};
use crate::{content::Catalog, ui::UiFrame};
use egui::{Align2, Color32, FontId, Stroke, Vec2};

pub(super) fn draw(root: &mut egui::Ui, frame: &UiFrame<'_>, catalog: &Catalog) {
    if let Some(chat) = frame.chat {
        crate::ui::chat::draw(root, chat);
    }
    let viewport = root.max_rect();
    let center = viewport.center();
    let painter = root.painter().clone();
    for (from, to) in [
        (
            center + Vec2::new(-12.0, 0.0),
            center + Vec2::new(-4.0, 0.0),
        ),
        (center + Vec2::new(4.0, 0.0), center + Vec2::new(12.0, 0.0)),
        (
            center + Vec2::new(0.0, -12.0),
            center + Vec2::new(0.0, -4.0),
        ),
        (center + Vec2::new(0.0, 4.0), center + Vec2::new(0.0, 12.0)),
    ] {
        painter.line_segment([from, to], Stroke::new(3.5, Color32::from_black_alpha(180)));
        painter.line_segment([from, to], Stroke::new(1.8, Color32::WHITE));
    }
    let compact = viewport.width() < 800.0 || viewport.height() < 500.0;
    let side = ((viewport.width() - 42.0) / 9.0 - 4.0).min(if compact { 42.0 } else { 54.0 });
    let hotbar_width = 9.0 * side + 8.0 * 4.0 + 10.0;
    let hotbar = egui::Rect::from_min_size(
        egui::pos2(
            viewport.center().x - hotbar_width * 0.5,
            viewport.bottom() - side - 22.0,
        ),
        Vec2::new(hotbar_width, side + 10.0),
    );
    root.scope_builder(egui::UiBuilder::new().max_rect(hotbar), |ui| {
        egui::Frame::new()
            .fill(Color32::from_black_alpha(165))
            .corner_radius(egui::CornerRadius::same(7))
            .inner_margin(egui::Margin::same(5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for index in 0..9u8 {
                        let _ = slot::show(
                            ui,
                            index,
                            frame.inventory[usize::from(index)].as_ref(),
                            catalog,
                            SlotStyle {
                                side,
                                selected: frame.selected_slot == usize::from(index),
                                highlighted: false,
                                dimmed: false,
                                hotbar: true,
                            },
                        );
                    }
                });
            });
    });
    if let Some(status) = frame.status {
        let message: String = status.chars().take(90).collect();
        let position = egui::pos2(viewport.center().x, hotbar.top() - 13.0);
        let status_rect = egui::Rect::from_center_size(
            position,
            Vec2::new(
                (message.len() as f32 * 7.0 + 18.0).min(viewport.width() - 16.0),
                25.0,
            ),
        );
        painter.rect_filled(status_rect, 5.0, Color32::from_black_alpha(190));
        painter.text(
            position,
            Align2::CENTER_CENTER,
            message,
            FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
    if let Some(debug) = frame.debug {
        let line = format!(
            "{:.0} FPS  /  {:.1} ms  /  {} visible  /  {} cached",
            debug.fps, debug.frame_ms, debug.visible_chunks, debug.cached_chunks
        );
        painter.text(
            viewport.left_top() + Vec2::new(12.0, 12.0),
            Align2::LEFT_TOP,
            line,
            FontId::monospace(12.0),
            Color32::WHITE,
        );
    }
}
