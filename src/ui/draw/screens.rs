//! Screen-specific menu drawing.

use super::*;

impl UiBuilder<'_> {
    pub(super) fn draw_inventory(&mut self, frame: &UiFrame<'_>, layout: &UiLayout) {
        self.screen_dim();
        let panel = layout.inventory_panel();
        self.panel(panel);
        let compact = panel.width < 700.0 * self.scale;
        self.text(
            "BLOCK CATALOG",
            panel.x + (if compact { 20.0 } else { 30.0 }) * self.scale,
            panel.y + (if compact { 14.0 } else { 28.0 }) * self.scale,
            if compact { 0.96 } else { 1.2 },
            TEXT,
            32,
        );
        let current_slot = frame.selected_slot.min(8) + 1;
        let hint = format!("CHOOSE A BLOCK FOR SLOT {current_slot}");
        if compact {
            self.text(
                &hint,
                panel.x + 20.0 * self.scale,
                panel.y + 46.0 * self.scale,
                0.66,
                GOLD,
                40,
            );
        } else {
            self.text(
                "CREATIVE BUILDING",
                panel.x + 30.0 * self.scale,
                panel.y + 60.0 * self.scale,
                0.76,
                MUTED,
                32,
            );
            self.text(
                &hint,
                panel.x + panel.width - 30.0 * self.scale - text_width(&hint, self.scale, 0.78),
                panel.y + 36.0 * self.scale,
                0.78,
                GOLD,
                40,
            );
        }
        for block in 1u8..=8 {
            let Some(card) = layout.rect(UiControl::CatalogBlock(block)) else {
                continue;
            };
            let selected = frame.catalog_selection == block;
            let hovered = frame.hovered == Some(UiControl::CatalogBlock(block));
            self.rounded_panel(
                card,
                if selected {
                    [0.12, 0.17, 0.12, 0.95]
                } else {
                    [0.075, 0.09, 0.09, 0.92]
                },
                if selected {
                    GOLD
                } else if hovered {
                    [0.65, 0.77, 0.52, 0.9]
                } else {
                    EDGE
                },
                if selected {
                    2.2 * self.scale
                } else {
                    1.0 * self.scale
                },
            );
            let compact_card = card.height < 70.0 * self.scale;
            let swatch_size = ((if compact_card { 22.0 } else { 42.0 }) * self.scale)
                .min(card.height * 0.44)
                .min(card.width * 0.35);
            let swatch = UiRect {
                x: card.x + (if compact_card { 8.0 } else { 18.0 }) * self.scale,
                y: card.y + (card.height - swatch_size) * 0.5,
                width: swatch_size,
                height: swatch_size,
            };
            self.rect(
                swatch.x + 3.0 * self.scale,
                swatch.y,
                swatch.width,
                swatch.height,
                block_color(block),
            );
            self.rect(
                swatch.x,
                swatch.y + swatch.height * 0.65,
                swatch.width,
                swatch.height * 0.35,
                darken(block_color(block), 0.72),
            );
            self.text(
                block_name(block),
                swatch.x + swatch.width + (if compact_card { 7.0 } else { 14.0 }) * self.scale,
                card.y + card.height * (if compact_card { 0.32 } else { 0.44 }),
                if compact_card {
                    0.64
                } else if block == 8 {
                    0.58
                } else {
                    0.88
                },
                TEXT,
                18,
            );
            if !compact_card {
                self.text(
                    "PLACEABLE",
                    swatch.x + swatch.width + 14.0 * self.scale,
                    card.y + card.height * 0.70,
                    0.58,
                    MUTED,
                    18,
                );
            }
        }
        let footer = "1-9 CHOOSES SLOT  /  E OR ESC CLOSES";
        self.center_text(
            footer,
            panel.x + panel.width * 0.5,
            panel.y + panel.height - (if compact { 32.0 } else { 46.0 }) * self.scale,
            if compact { 0.65 } else { 0.76 },
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
            (UiControl::Exit, "EXIT GAME"),
        ] {
            if let Some(rect) = layout.rect(control) {
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
