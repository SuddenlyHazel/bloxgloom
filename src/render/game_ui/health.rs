//! Live egui health presentation; button clicks request server decisions.
use super::Intent;
use crate::ui::{UiControl, UiFrame};
use egui::{Color32, RichText, Vec2};

pub(super) fn hud(root: &mut egui::Ui, frame: &UiFrame<'_>) {
    let Some(health) = frame.health else { return };
    let viewport = root.max_rect();
    let origin = egui::pos2(viewport.left() + 24., viewport.bottom() - 92.);
    let bar = egui::Rect::from_min_size(origin, Vec2::new(180., 12.));
    let painter = root.painter();
    painter.rect_filled(bar, 3., Color32::from_black_alpha(180));
    painter.rect_filled(
        egui::Rect::from_min_size(
            origin,
            Vec2::new(180. * health.current as f32 / health.max as f32, 12.),
        ),
        3.,
        Color32::from_rgb(191, 38, 48),
    );
    painter.text(
        origin - Vec2::new(0., 6.),
        egui::Align2::LEFT_BOTTOM,
        format!("HEALTH {} / {}", health.current, health.max),
        egui::FontId::monospace(13.),
        Color32::WHITE,
    );
}

pub(super) fn death(root: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    let viewport = root.max_rect();
    root.painter()
        .rect_filled(viewport, 0., Color32::from_black_alpha(205));
    let width = (viewport.width() - 24.).min(340.);
    let card = egui::Rect::from_center_size(viewport.center(), Vec2::new(width, 176.));
    root.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("YOU DIED")
                    .size(28.)
                    .color(Color32::from_rgb(235, 178, 174)),
            );
            ui.add_space(8.);
            ui.label("Respawn when you are ready.");
            ui.label("Your inventory is kept.");
            ui.add_space(16.);
            let eligible = frame.health.is_some_and(|health| !health.alive);
            if ui
                .add_enabled(
                    eligible,
                    egui::Button::new("RESPAWN").min_size(Vec2::new(200., 44.)),
                )
                .clicked()
            {
                intents.push(Intent::Control(UiControl::Respawn));
            }
        });
    });
}
#[cfg(test)]
mod tests;
