//! Compact native health HUD and explicit manual respawn.
use super::*;
impl UiBuilder<'_> {
    pub(super) fn draw_health(&mut self, frame: &UiFrame<'_>) {
        let Some(health) = frame.health else {
            return;
        };
        let x = 24. * self.scale;
        let y = self.height - 92. * self.scale;
        let width = 180. * self.scale;
        self.rect(x, y, width, 12. * self.scale, [0.03, 0.04, 0.04, 0.8]);
        self.rect(
            x,
            y,
            width * health.current as f32 / health.max as f32,
            12. * self.scale,
            [0.75, 0.15, 0.19, 0.95],
        );
        self.text(
            &format!("HEALTH {} / {}", health.current, health.max),
            x,
            y - 21. * self.scale,
            0.65,
            [0.96, 0.96, 0.92, 1.],
            32,
        );
    }
    pub(super) fn draw_death(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.rect(0., 0., self.width, self.height, [0.025, 0.025, 0.035, 0.78]);
        self.center_text(
            "YOU DIED",
            self.width * 0.5,
            self.height * 0.5 - 35. * self.scale,
            1.3,
            [0.92, 0.7, 0.68, 1.],
        );
        self.center_text(
            "RESPAWN WHEN YOU ARE READY",
            self.width * 0.5,
            self.height * 0.5,
            0.55,
            [0.8, 0.84, 0.82, 1.],
        );
        if let Some(rect) = layout.rect(UiControl::Respawn) {
            self.button(
                rect,
                "RESPAWN",
                frame.hovered == Some(UiControl::Respawn),
                false,
            );
        }
    }
}
