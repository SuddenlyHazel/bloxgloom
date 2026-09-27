use super::*;
use bloxgloom_host_api::actions::Widget;
impl UiBuilder<'_> {
    pub(super) fn draw_actions(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        let Some(composition) = frame.action_panel.as_ref().filter(|p| p.validate().is_ok()) else {
            return;
        };
        self.screen_dim();
        let panel = layout.inventory_panel();
        self.panel(panel);
        self.text(
            &composition.title,
            panel.x + 20.0 * self.scale,
            panel.y + 20.0 * self.scale,
            0.9,
            TEXT,
            32,
        );
        for (i, widget) in composition.widgets.iter().enumerate() {
            let row = layout.action_row(i, composition.widgets.len());
            match widget {
                Widget::Label(text) => {
                    self.text(text, row.x, row.y + 6.0 * self.scale, 0.6, MUTED, 48);
                }
                Widget::Button { label, tooltip, .. } => {
                    let hovered = frame.hovered == Some(UiControl::Action(i as u8));
                    self.rounded_panel(
                        row,
                        [0.08, 0.12, 0.1, 1.0],
                        if hovered { GOLD } else { EDGE },
                        self.scale,
                    );
                    self.center_text(
                        label,
                        row.x + row.width * 0.5,
                        row.y + (row.height - 10.0 * self.scale) * 0.5,
                        0.65,
                        TEXT,
                    );
                    if hovered {
                        self.text(
                            tooltip,
                            panel.x + 20.0 * self.scale,
                            panel.y + panel.height - 52.0 * self.scale,
                            0.48,
                            GOLD,
                            64,
                        );
                    }
                }
            }
        }
        self.center_text(
            "ESC: CLOSE / TAB: SELECT / ENTER: ACTIVATE",
            panel.x + panel.width * 0.5,
            panel.y + panel.height - 25.0 * self.scale,
            0.5,
            MUTED,
        );
    }
}
