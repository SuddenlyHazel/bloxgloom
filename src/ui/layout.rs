//! Screen-space control geometry shared by rendering and input.

use super::inventory_search::search_rect;
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
            UiScreen::Playing | UiScreen::Package | UiScreen::Joining | UiScreen::JoinFailed => {}
            UiScreen::Inventory => layout.add_inventory(),
            UiScreen::Container | UiScreen::Actions => {}
            UiScreen::Admin => layout.add_admin(),
            UiScreen::Pause => layout.add_pause(),
            UiScreen::Settings | UiScreen::Graphics => layout.add_settings(),
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

    pub(crate) fn binding_hit(&self, x: f32, y: f32) -> Option<UiControl> {
        (0..8)
            .map(UiControl::AdminBindingRow)
            .find(|&control| self.rect(control).is_some_and(|rect| rect.contains(x, y)))
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
        self.push(UiControl::InventorySearch, search_rect(panel, self.scale));
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

    pub fn with_container(mut self, screen: Option<&bloxgloom_host_api::InventoryScreen>) -> Self {
        if self.screen == UiScreen::Container
            && let Some(screen) = screen
        {
            self.add_container(screen);
        }
        self
    }

    pub fn with_actions(mut self, panel: Option<&bloxgloom_host_api::actions::Panel>) -> Self {
        if self.screen == UiScreen::Actions
            && let Some(panel) = panel.filter(|p| p.validate().is_ok())
        {
            for (i, widget) in panel.widgets.iter().enumerate() {
                if matches!(widget, bloxgloom_host_api::actions::Widget::Button { .. }) {
                    self.push(
                        UiControl::Action(i as u8),
                        self.action_row(i, panel.widgets.len()),
                    );
                }
            }
        }
        self
    }
    pub(super) fn action_row(&self, index: usize, count: usize) -> UiRect {
        let panel = self.inventory_panel();
        let pitch =
            ((panel.height - 110.0 * self.scale) / count.max(1) as f32).min(48.0 * self.scale);
        UiRect {
            x: panel.x + 20.0 * self.scale,
            y: panel.y + 55.0 * self.scale + index as f32 * pitch,
            width: panel.width - 40.0 * self.scale,
            height: pitch - 4.0 * self.scale,
        }
    }

    fn add_container(&mut self, screen: &bloxgloom_host_api::InventoryScreen) {
        let panel = self.kiln_panel();
        let gap = 4.0 * self.scale;
        let rows = f32::from(screen.slots.div_ceil(screen.columns));
        let header = if screen.status.is_empty() { 58.0 } else { 84.0 };
        let size = ((panel.width - 36.0 * self.scale - gap * 8.0) / 9.0)
            .min((panel.height - (header + 64.0) * self.scale - gap * (rows + 2.0)) / (rows + 4.0))
            .max(8.0);
        let x = panel.x + (panel.width - size * 9.0 - gap * 8.0) * 0.5;
        let top = panel.y + header * self.scale;
        let storage_pitch = (size + gap) * 9.0 / f32::from(screen.columns);
        let storage_x =
            panel.x + (panel.width - size - f32::from(screen.columns - 1) * storage_pitch) * 0.5;
        for slot in 0..screen.slots {
            self.push(
                UiControl::KilnSlot(slot),
                UiRect {
                    x: storage_x + f32::from(slot % screen.columns) * storage_pitch,
                    y: top + f32::from(slot / screen.columns) * (size + gap),
                    width: size,
                    height: size,
                },
            );
        }
        let top = top + rows * (size + gap) + 24.0 * self.scale;
        for slot in 0..36u8 {
            let (row, col) = if slot < 9 {
                (3, slot)
            } else {
                ((slot - 9) / 9, (slot - 9) % 9)
            };
            self.push(
                UiControl::InventorySlot(slot),
                UiRect {
                    x: x + f32::from(col) * (size + gap),
                    y: top + f32::from(row) * (size + gap),
                    width: size,
                    height: size,
                },
            );
        }
    }

    pub(super) fn kiln_panel(&self) -> UiRect {
        centered_panel(
            self.width,
            self.height,
            720.0 * self.scale,
            540.0 * self.scale,
            12.0 * self.scale,
        )
    }

    fn add_pause(&mut self) {
        let panel = self.pause_panel();
        let button_width = panel.width * 0.72;
        let compact = panel.height < 360.0 * self.scale;
        let button_height = if compact {
            34.0 * self.scale
        } else {
            (42.0 * self.scale).clamp(34.0, 50.0)
        };
        let gap = if compact {
            6.0 * self.scale
        } else {
            10.0 * self.scale
        };
        let first_y = if compact {
            panel.y + 62.0 * self.scale
        } else {
            panel.y + panel.height * 0.30
        };
        let x = panel.x + (panel.width - button_width) * 0.5;
        for (index, control) in [
            UiControl::Resume,
            UiControl::OpenSettings,
            UiControl::OpenAdmin,
            UiControl::Exit,
        ]
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

    fn add_admin(&mut self) {
        let panel = self.admin_panel();
        self.push(
            UiControl::AdminBindings,
            UiRect {
                x: panel.x + panel.width - 145.0 * self.scale,
                y: panel.y + 14.0 * self.scale,
                width: 121.0 * self.scale,
                height: 30.0 * self.scale,
            },
        );
        let binding_pitch = ((panel.height - 140.0 * self.scale) / 8.0).min(44.0 * self.scale);
        for index in 0..8 {
            self.push(
                UiControl::AdminBindingRow(index),
                UiRect {
                    x: panel.x + 24.0 * self.scale,
                    y: panel.y + 78.0 * self.scale + index as f32 * binding_pitch,
                    width: panel.width - 48.0 * self.scale,
                    height: binding_pitch - 5.0 * self.scale,
                },
            );
        }
        let gap = 8.0 * self.scale;
        let cell_width = (panel.width - 48.0 * self.scale - 5.0 * gap) / 6.0;
        let cell_height = ((panel.height - 194.0 * self.scale - 3.0 * gap) / 4.0).max(22.0);
        for index in 0..24u8 {
            let col = index as usize % 6;
            let row = index as usize / 6;
            self.push(
                UiControl::AdminItem(index),
                UiRect {
                    x: panel.x + 24.0 * self.scale + col as f32 * (cell_width + gap),
                    y: panel.y + 72.0 * self.scale + row as f32 * (cell_height + gap),
                    width: cell_width,
                    height: cell_height,
                },
            );
        }
        let bottom = panel.y + panel.height - 44.0 * self.scale;
        for (control, x, width) in [
            (
                UiControl::AdminPrev,
                panel.x + 24.0 * self.scale,
                78.0 * self.scale,
            ),
            (
                UiControl::AdminNext,
                panel.x + 112.0 * self.scale,
                78.0 * self.scale,
            ),
            (
                UiControl::AdminRun,
                panel.x + panel.width - 116.0 * self.scale,
                92.0 * self.scale,
            ),
        ] {
            self.push(
                control,
                UiRect {
                    x,
                    y: bottom,
                    width,
                    height: 30.0 * self.scale,
                },
            );
        }
    }

    fn add_settings(&mut self) {
        let panel = self.settings_panel();
        let compact = panel.height < 500.0 * self.scale;
        self.push(
            UiControl::ToggleSettingsPage,
            UiRect {
                x: panel.x + panel.width - 154.0 * self.scale,
                y: panel.y + (if compact { 12.0 } else { 24.0 }) * self.scale,
                width: 130.0 * self.scale,
                height: 30.0 * self.scale,
            },
        );
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
        let settings: &[SettingId] = if self.screen == UiScreen::Graphics {
            &[
                SettingId::PostProcessing,
                SettingId::Exposure,
                SettingId::Bloom,
                SettingId::BloomStrength,
                SettingId::Characters,
            ]
        } else {
            &[
                SettingId::Sensitivity,
                SettingId::FieldOfView,
                SettingId::ViewDistance,
                SettingId::UiScale,
                SettingId::Lighting,
            ]
        };
        for (index, &setting) in settings.iter().enumerate() {
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
        if self.screen == UiScreen::Settings {
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
        }
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

    pub(super) fn admin_panel(&self) -> UiRect {
        centered_panel(
            self.width,
            self.height,
            820.0 * self.scale,
            560.0 * self.scale,
            14.0 * self.scale,
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
