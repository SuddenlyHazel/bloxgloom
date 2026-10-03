//! Controller events share egui's pointer path; the cursor is drawn above menus.
use super::GameUi;
impl GameUi {
    pub(in crate::render) fn controller_pointer(
        &mut self,
        position: Option<[f32; 2]>,
        button: Option<(bool, bool)>,
        scroll: [f32; 2],
    ) {
        self.controller_cursor.append_events(
            &mut self.input.egui_input_mut().events,
            self.context.pixels_per_point(),
            position,
            button,
            scroll,
        );
    }
}

#[derive(Default)]
pub(super) struct Pointer {
    pub position: egui::Pos2,
    pub visible: bool,
}
impl Pointer {
    fn append_events(
        &mut self,
        events: &mut Vec<egui::Event>,
        scale: f32,
        position: Option<[f32; 2]>,
        button: Option<(bool, bool)>,
        scroll: [f32; 2],
    ) {
        // Include both HiDPI and the game's UI zoom when converting window pixels.
        if let Some([x, y]) = position {
            self.position = egui::pos2(x / scale, y / scale);
            self.visible = x >= 0.0 && y >= 0.0;
            events.push(egui::Event::PointerMoved(self.position));
        }
        if let Some((pressed, secondary)) = button {
            events.push(egui::Event::PointerButton {
                pos: self.position,
                button: if secondary {
                    egui::PointerButton::Secondary
                } else {
                    egui::PointerButton::Primary
                },
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if scroll != [0.0, 0.0] {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: egui::vec2(scroll[0] / scale, scroll[1] / scale),
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
}

pub(crate) fn draw_cursor(context: &egui::Context, position: egui::Pos2) {
    let painter = context.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("controller-cursor"),
    ));
    painter.circle_filled(position, 6.0, egui::Color32::from_rgb(28, 30, 36));
    painter.circle_stroke(position, 4.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
    painter.circle_filled(position, 1.0, egui::Color32::WHITE);
}

#[cfg(test)]
mod tests;
