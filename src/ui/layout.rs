//! Screen-space control geometry shared by rendering and input.

use super::types::{SettingId, UiControl, UiRect, UiScreen};

#[derive(Clone, Debug)]
pub(super) struct HitRect {
    pub(super) control: UiControl,
    pub(super) rect: UiRect,
}

/// Physical-pixel rectangles for all controls, shared by rendering and input handling.
#[derive(Clone, Debug)]
pub struct UiLayout {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub(super) screen: UiScreen,
    pub(super) hits: Vec<HitRect>,
}

impl UiLayout {
    pub fn new(width: u32, height: u32, scale: f32, screen: UiScreen) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let scale = effective_ui_scale(width, height, scale);
        let mut layout = Self {
            width,
            height,
            scale,
            screen,
            hits: Vec::with_capacity(20),
        };
        layout.add_hotbar();
        match screen {
            UiScreen::Playing => {}
            UiScreen::Inventory => layout.add_inventory(),
            UiScreen::Pause => layout.add_pause(),
            UiScreen::Settings => layout.add_settings(),
        }
        layout
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<UiControl> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(x, y))
            .map(|hit| hit.control)
    }

    pub fn rect(&self, control: UiControl) -> Option<UiRect> {
        self.hits
            .iter()
            .find(|hit| hit.control == control)
            .map(|hit| hit.rect)
    }

    fn push(&mut self, control: UiControl, rect: UiRect) {
        self.hits.push(HitRect { control, rect });
    }

    fn add_hotbar(&mut self) {
        let gap = (6.0 * self.scale).max(3.0);
        let proposed = 54.0 * self.scale;
        let slot = proposed.min((self.width as f32 - 32.0).max(9.0) / 9.0 - gap);
        let slot = slot.max(24.0);
        let total = slot * 9.0 + gap * 8.0;
        let x0 = (self.width as f32 - total) * 0.5;
        let y = (self.height as f32 - slot - 22.0 * self.scale).max(4.0);
        for slot_index in 0..9 {
            self.push(
                UiControl::HotbarSlot(slot_index),
                UiRect {
                    x: x0 + slot_index as f32 * (slot + gap),
                    y,
                    width: slot,
                    height: slot,
                },
            );
        }
    }

    fn add_inventory(&mut self) {
        let panel = self.inventory_panel();
        let compact = panel.height < 380.0 * self.scale;
        let gap = (if compact { 6.0 } else { 8.0 }) * self.scale;
        let size = (if compact { 48.0 } else { 60.0 } * self.scale)
            .min((panel.width - 32.0 * self.scale - gap * 8.0) / 9.0)
            .min((panel.height - 90.0 * self.scale - gap * 3.0) / 4.0);
        let x0 = panel.x + (panel.width - (9.0 * size + 8.0 * gap)) * 0.5;
        let y0 = panel.y + (if compact { 52.0 } else { 82.0 }) * self.scale;
        for index in 0..36u8 {
            let (row, col) = if index < 9 {
                (3, index as usize)
            } else {
                ((index as usize - 9) / 9, (index as usize - 9) % 9)
            };
            self.push(
                UiControl::InventorySlot(index),
                UiRect {
                    x: x0 + col as f32 * (size + gap),
                    y: y0
                        + row as f32 * (size + gap)
                        + if row == 3 { 9.0 * self.scale } else { 0.0 },
                    width: size,
                    height: size,
                },
            );
        }
    }

    fn add_pause(&mut self) {
        let panel = self.pause_panel();
        let button_width = panel.width * 0.72;
        let compact = panel.height < 360.0 * self.scale;
        let button_height = if compact {
            38.0 * self.scale
        } else {
            (48.0 * self.scale).clamp(36.0, 56.0)
        };
        let gap = if compact {
            8.0 * self.scale
        } else {
            14.0 * self.scale
        };
        let first_y = if compact {
            panel.y + 86.0 * self.scale
        } else {
            panel.y + panel.height * 0.38
        };
        let x = panel.x + (panel.width - button_width) * 0.5;
        for (index, control) in [UiControl::Resume, UiControl::OpenSettings, UiControl::Exit]
            .into_iter()
            .enumerate()
        {
            self.push(
                control,
                UiRect {
                    x,
                    y: first_y + index as f32 * (button_height + gap),
                    width: button_width,
                    height: button_height,
                },
            );
        }
    }

    fn add_settings(&mut self) {
        let panel = self.settings_panel();
        let compact = panel.height < 500.0 * self.scale;
        let row_height = if compact {
            36.0 * self.scale
        } else {
            (52.0 * self.scale).clamp(42.0, 58.0)
        };
        let top = panel.y + (if compact { 74.0 } else { 116.0 }) * self.scale;
        let label_width = panel.width * if compact { 0.46 } else { 0.49 };
        let control_x = panel.x + label_width;
        let button_width = (42.0 * self.scale).clamp(34.0, 48.0);
        let value_width = if compact {
            72.0 * self.scale
        } else {
            (106.0 * self.scale).clamp(72.0, 128.0)
        };
        for (index, setting) in [
            SettingId::Sensitivity,
            SettingId::FieldOfView,
            SettingId::ViewDistance,
            SettingId::UiScale,
            SettingId::Lighting,
        ]
        .into_iter()
        .enumerate()
        {
            let y = top + index as f32 * row_height;
            self.push(
                UiControl::Decrease(setting),
                UiRect {
                    x: control_x,
                    y,
                    width: button_width,
                    height: row_height - 8.0 * self.scale,
                },
            );
            self.push(
                UiControl::Increase(setting),
                UiRect {
                    x: control_x + button_width + value_width,
                    y,
                    width: button_width,
                    height: row_height - 8.0 * self.scale,
                },
            );
        }
        self.push(
            UiControl::ToggleFullscreen,
            UiRect {
                x: panel.x + panel.width * if compact { 0.45 } else { 0.22 },
                y: panel.y + panel.height - (if compact { 46.0 } else { 104.0 }) * self.scale,
                width: panel.width * if compact { 0.52 } else { 0.56 },
                height: if compact {
                    32.0 * self.scale
                } else {
                    (48.0 * self.scale).clamp(36.0, 56.0)
                },
            },
        );
        self.push(
            UiControl::Back,
            UiRect {
                x: panel.x + 18.0 * self.scale,
                y: panel.y + panel.height - (if compact { 46.0 } else { 58.0 }) * self.scale,
                width: if compact {
                    panel.width * 0.34
                } else {
                    (116.0 * self.scale).clamp(88.0, 142.0)
                },
                height: if compact {
                    32.0 * self.scale
                } else {
                    (38.0 * self.scale).clamp(30.0, 44.0)
                },
            },
        );
    }

    pub(super) fn inventory_panel(&self) -> UiRect {
        let desired_height = if self.height < 500 { 332.0 } else { 420.0 };
        centered_panel(
            self.width,
            self.height,
            760.0 * self.scale,
            desired_height * self.scale,
            14.0 * self.scale,
        )
    }

    pub(super) fn pause_panel(&self) -> UiRect {
        let desired_height = if self.height < 500 { 280.0 } else { 420.0 };
        centered_panel(
            self.width,
            self.height,
            420.0 * self.scale,
            desired_height * self.scale,
            14.0 * self.scale,
        )
    }

    pub(super) fn settings_panel(&self) -> UiRect {
        centered_panel(
            self.width,
            self.height,
            720.0 * self.scale,
            540.0 * self.scale,
            14.0,
        )
    }
}

pub(super) fn effective_ui_scale(width: u32, height: u32, requested: f32) -> f32 {
    let requested = if requested.is_finite() {
        requested.clamp(0.75, 2.0)
    } else {
        1.0
    };
    let fit = (width as f32 / 640.0).min(height as f32 / 360.0);
    requested.min(fit.max(0.75))
}

fn centered_panel(
    width: u32,
    height: u32,
    desired_width: f32,
    desired_height: f32,
    margin: f32,
) -> UiRect {
    let panel_width = desired_width.min((width as f32 - margin * 2.0).max(1.0));
    let panel_height = desired_height.min((height as f32 - margin * 2.0).max(1.0));
    UiRect {
        x: (width as f32 - panel_width) * 0.5,
        y: (height as f32 - panel_height) * 0.5,
        width: panel_width,
        height: panel_height,
    }
}
