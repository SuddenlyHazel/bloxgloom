//! Connection status, byte progress, cache reuse and retry/cancel controls.
use super::{EDGE, Intent, PANEL, UiFrame, UiScreen, title};
use egui::{Color32, Margin, Stroke, Vec2};

pub(super) fn draw(root: &mut egui::Ui, frame: &UiFrame<'_>, intents: &mut Vec<Intent>) {
    let viewport = root.max_rect();
    root.painter()
        .rect_filled(viewport, 0.0, Color32::from_rgb(19, 29, 33));
    let card = egui::Rect::from_center_size(
        viewport.center(),
        Vec2::new(
            (viewport.width() - 24.0).min(640.0),
            (viewport.height() - 24.0).min(520.0),
        ),
    );
    root.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
        egui::Frame::new()
            .fill(PANEL)
            .stroke(Stroke::new(2.0, EDGE))
            .corner_radius(egui::CornerRadius::same(11))
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(card.width() - 32.0);
                title(
                    ui,
                    if frame.screen == UiScreen::JoinFailed {
                        "Unable to join"
                    } else {
                        "Joining"
                    },
                    "Enter a server address; the session is prepared before world state is shown.",
                );
                let mut address = frame.join_address.unwrap_or_default().to_owned();
                if ui
                    .add_sized(
                        [ui.available_width(), 32.0],
                        egui::TextEdit::singleline(&mut address).char_limit(256),
                    )
                    .changed()
                {
                    intents.push(Intent::JoinAddress(address));
                }
                ui.add_space(9.0);
                if let Some(progress) = frame.join_progress {
                    if progress.cached {
                        ui.label("Using verified package already in cache — no download needed.");
                    } else {
                        let fraction = progress.received as f32 / progress.total.max(1) as f32;
                        ui.add(egui::ProgressBar::new(fraction).text(format!(
                            "{:.0}% · {:.1} / {:.1} KiB",
                            fraction * 100.0,
                            progress.received as f64 / 1024.0,
                            progress.total as f64 / 1024.0,
                        )));
                    }
                    ui.add_space(6.0);
                }
                egui::ScrollArea::vertical()
                    .id_salt("join-status")
                    .max_height(
                        card.height()
                            - if frame.join_progress.is_some() {
                                220.0
                            } else {
                                180.0
                            },
                    )
                    .show(ui, |ui| {
                        ui.label(frame.status.unwrap_or("Preparing…"));
                    });
                ui.add_space(9.0);
                if ui
                    .add_sized(
                        [ui.available_width(), 35.0],
                        egui::Button::new(if frame.screen == UiScreen::JoinFailed {
                            "Retry"
                        } else {
                            "Cancel"
                        }),
                    )
                    .clicked()
                {
                    intents.push(Intent::JoinAction);
                }
            });
    });
}
