//! Screen-specific menu drawing.

use super::*;
mod admin;

impl UiBuilder<'_> {
    pub(super) fn draw_inventory(
        &mut self,
        frame: &UiFrame<'_>,
        layout: &UiLayout,
        catalog: &crate::content::Catalog,
    ) {
        self.screen_dim();
        let panel = layout.inventory_panel();
        self.panel(panel);
        let compact = panel.height < 380.0 * self.scale;
        self.text(
            "INVENTORY",
            panel.x + 22.0 * self.scale,
            panel.y + (if compact { 12.0 } else { 25.0 }) * self.scale,
            if compact { 1.0 } else { 1.25 },
            TEXT,
            24,
        );
        if !compact {
            self.text(
                "36 SLOTS  /  128 PER STACK",
                panel.x + 24.0 * self.scale,
                panel.y + 55.0 * self.scale,
                0.66,
                MUTED,
                40,
            );
        }
        for index in 0..36u8 {
            let Some(rect) = layout.rect(UiControl::InventorySlot(index)) else {
                continue;
            };
            let selected = frame.inventory_source == Some(index);
            let hotbar = index < 9;
            let hovered = frame.hovered == Some(UiControl::InventorySlot(index));
            self.rounded_panel(
                rect,
                if selected {
                    [0.17, 0.21, 0.13, 0.97]
                } else {
                    [0.055, 0.075, 0.073, 0.95]
                },
                if selected {
                    GOLD
                } else if hovered {
                    [0.65, 0.77, 0.52, 0.95]
                } else if hotbar {
                    [0.43, 0.51, 0.39, 0.95]
                } else {
                    EDGE
                },
                if selected {
                    2.3 * self.scale
                } else {
                    1.0 * self.scale
                },
            );
            if let Some(stack) = frame.inventory[index as usize].as_ref() {
                let swatch = inset(rect, rect.width * 0.23, rect.height * 0.23);
                self.draw_item_swatch(swatch, stack.item, catalog);
                let count = stack.count.to_string();
                self.text(
                    &count,
                    rect.x + 4.0 * self.scale,
                    rect.y + rect.height - 14.0 * self.scale,
                    if compact { 0.52 } else { 0.64 },
                    TEXT,
                    3,
                );
            }
            if hotbar {
                self.text(
                    &(index + 1).to_string(),
                    rect.x + rect.width - 15.0 * self.scale,
                    rect.y + 4.0 * self.scale,
                    0.5,
                    GOLD,
                    1,
                );
            }
        }
        let footer = if compact {
            "L: MOVE  R: HALF  Q: DROP  E: CLOSE"
        } else {
            "SOURCE THEN DEST  /  RIGHT: HALF  /  Q: DROP  /  E: CLOSE"
        };
        self.center_text(
            footer,
            panel.x + panel.width * 0.5,
            panel.y + panel.height - (if compact { 21.0 } else { 31.0 }) * self.scale,
            if compact { 0.52 } else { 0.63 },
            MUTED,
        );
    }

    pub(super) fn draw_pause(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.screen_dim();
        let panel = layout.pause_panel();
        self.panel(panel);
        let compact = panel.height < 360.0 * self.scale;
        self.center_text(
            "BLOXGLOOM",
            panel.x + panel.width * 0.5,
            panel.y + (if compact { 42.0 } else { 56.0 }) * self.scale,
            if compact { 1.2 } else { 1.55 },
            TEXT,
        );
        if !compact {
            self.center_text(
                "PAUSED",
                panel.x + panel.width * 0.5,
                panel.y + 92.0 * self.scale,
                0.82,
                MUTED,
            );
        }
        for (control, label) in [
            (UiControl::Resume, "RESUME"),
            (UiControl::OpenSettings, "SETTINGS"),
            (UiControl::OpenAdmin, "ADMIN TOOLS"),
            (UiControl::Exit, "EXIT GAME"),
        ] {
            if (control != UiControl::OpenAdmin || frame.admin_enabled)
                && let Some(rect) = layout.rect(control)
            {
                self.button(rect, label, frame.hovered == Some(control), false);
            }
        }
        self.center_text(
            "ESC TO RETURN",
            panel.x + panel.width * 0.5,
            panel.y + panel.height - (if compact { 22.0 } else { 30.0 }) * self.scale,
            if compact { 0.62 } else { 0.7 },
            MUTED,
        );
    }

    pub(super) fn draw_settings(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.screen_dim();
        let panel = layout.settings_panel();
        self.panel(panel);
        let compact = panel.height < 500.0 * self.scale;
        self.text(
            "SETTINGS",
            panel.x + 30.0 * self.scale,
            panel.y + (if compact { 14.0 } else { 28.0 }) * self.scale,
            if compact { 1.0 } else { 1.3 },
            TEXT,
            30,
        );
        if !compact {
            self.text(
                "LOCAL CLIENT OPTIONS",
                panel.x + 30.0 * self.scale,
                panel.y + 62.0 * self.scale,
                0.72,
                MUTED,
                36,
            );
        }
        let rows = [
            (
                SettingId::Sensitivity,
                "MOUSE SENSITIVITY",
                format!("{:.3}", frame.settings.sensitivity),
            ),
            (
                SettingId::FieldOfView,
                "FIELD OF VIEW",
                format!("{:.0} DEG", frame.settings.fov_degrees),
            ),
            (
                SettingId::ViewDistance,
                "VIEW DISTANCE",
                frame.settings.view_distance.to_string(),
            ),
            (
                SettingId::UiScale,
                if frame.settings.scale > self.scale + 0.01 {
                    "UI SCALE (FITTED)"
                } else {
                    "UI SCALE"
                },
                format!("{:.1}X", frame.settings.scale),
            ),
            (
                SettingId::Lighting,
                "LIGHTING",
                if frame.settings.bounced_gi {
                    "BOUNCED"
                } else {
                    "VOXEL"
                }
                .to_string(),
            ),
        ];
        let top = panel.y + (if compact { 74.0 } else { 116.0 }) * self.scale;
        let row_height = if compact {
            36.0 * self.scale
        } else {
            (52.0 * self.scale).clamp(42.0, 58.0)
        };
        let control_x = panel.x + panel.width * if compact { 0.46 } else { 0.49 };
        let minus_w = (42.0 * self.scale).clamp(34.0, 48.0);
        let value_w = if compact {
            72.0 * self.scale
        } else {
            (106.0 * self.scale).clamp(72.0, 128.0)
        };
        for (index, (setting, label, value)) in rows.iter().enumerate() {
            let y = top + index as f32 * row_height;
            self.text(
                label,
                panel.x + (if compact { 20.0 } else { 32.0 }) * self.scale,
                y + (if compact { 9.0 } else { 10.0 }) * self.scale,
                if compact { 0.68 } else { 0.82 },
                TEXT,
                24,
            );
            if let Some(rect) = layout.rect(UiControl::Decrease(*setting)) {
                self.button(
                    rect,
                    "-",
                    frame.hovered == Some(UiControl::Decrease(*setting)),
                    true,
                );
            }
            let value_rect = UiRect {
                x: control_x + minus_w,
                y,
                width: value_w,
                height: row_height - 8.0 * self.scale,
            };
            self.rounded_panel(
                value_rect,
                [0.035, 0.05, 0.05, 0.90],
                EDGE,
                1.0 * self.scale,
            );
            self.center_text(
                value,
                value_rect.x + value_rect.width * 0.5,
                value_rect.y + value_rect.height * 0.5,
                if compact { 0.60 } else { 0.78 },
                GOLD,
            );
            if let Some(rect) = layout.rect(UiControl::Increase(*setting)) {
                self.button(
                    rect,
                    "+",
                    frame.hovered == Some(UiControl::Increase(*setting)),
                    true,
                );
            }
        }
        if let Some(rect) = layout.rect(UiControl::ToggleFullscreen) {
            let label = if frame.settings.fullscreen {
                "FULLSCREEN: ON"
            } else {
                "FULLSCREEN: OFF"
            };
            self.button(
                rect,
                label,
                frame.hovered == Some(UiControl::ToggleFullscreen),
                compact,
            );
        }
        if let Some(rect) = layout.rect(UiControl::Back) {
            self.button(rect, "BACK", frame.hovered == Some(UiControl::Back), false);
        }
        if !compact {
            const HELP: &str = "ARROWS / +/- ADJUST  /  ESC BACK";
            self.text(
                HELP,
                panel.x + panel.width - 32.0 * self.scale - text_width(HELP, self.scale, 0.64),
                panel.y + panel.height - 46.0 * self.scale,
                0.64,
                MUTED,
                42,
            );
        }
    }
}
