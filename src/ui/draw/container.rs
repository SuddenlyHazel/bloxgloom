use super::*;
use bloxgloom_host_api::StatusFormat;

impl UiBuilder<'_> {
    pub(super) fn draw_container(
        &mut self,
        frame: &UiFrame<'_>,
        layout: &UiLayout,
        catalog: &Catalog,
    ) {
        self.screen_dim();
        let panel = layout.kiln_panel();
        self.panel(panel);
        let (Some(screen), Some(view)) = (&frame.container_screen, &frame.kiln) else {
            return;
        };
        self.text(
            &screen.title,
            panel.x + 18.0 * self.scale,
            panel.y + 13.0 * self.scale,
            0.85,
            TEXT,
            ((panel.width - 36.0 * self.scale) / (18.0 * self.scale * 0.85)) as usize,
        );
        let status_width = (panel.width - 36.0 * self.scale) / screen.status.len().max(1) as f32;
        for (index, (field, value)) in screen.status.iter().zip(&view.status).enumerate() {
            let x = panel.x + 18.0 * self.scale + index as f32 * status_width;
            let label = match field.format {
                StatusFormat::Number => format!("{} {}", field.label, value),
                StatusFormat::Milliseconds => {
                    format!("{} {:.1}S", field.label, *value as f32 / 1000.0)
                }
                StatusFormat::Progress => format!(
                    "{} {}%",
                    field.label,
                    u64::from(*value) * 100 / u64::from(field.maximum)
                ),
            };
            self.text(
                &label,
                x,
                panel.y + 37.0 * self.scale,
                0.5,
                GOLD,
                ((status_width - 8.0 * self.scale) / (18.0 * self.scale * 0.5)) as usize,
            );
            if field.format == StatusFormat::Progress {
                self.rect(
                    x,
                    panel.y + 52.0 * self.scale,
                    status_width - 12.0 * self.scale,
                    5.0 * self.scale,
                    EDGE,
                );
                self.rect(
                    x,
                    panel.y + 52.0 * self.scale,
                    (status_width - 12.0 * self.scale)
                        * (*value as f32 / field.maximum as f32).min(1.0),
                    5.0 * self.scale,
                    GOLD,
                );
            }
        }
        for group in &screen.groups {
            if group.first < screen.columns
                && let Some(rect) = layout.rect(UiControl::KilnSlot(group.first))
            {
                let right = screen
                    .groups
                    .iter()
                    .filter(|g| g.first > group.first && g.first < screen.columns)
                    .filter_map(|g| layout.rect(UiControl::KilnSlot(g.first)))
                    .map(|r| r.x)
                    .fold(panel.x + panel.width - 18.0 * self.scale, f32::min);
                self.text(
                    &group.label,
                    rect.x,
                    rect.y - 13.0 * self.scale,
                    0.45,
                    MUTED,
                    ((right - rect.x - 2.0 * self.scale) / (18.0 * self.scale * 0.45)) as usize,
                );
            }
        }
        if let Some(rect) = layout.rect(UiControl::InventorySlot(9)) {
            self.text(
                "INVENTORY",
                rect.x,
                rect.y - 17.0 * self.scale,
                0.55,
                MUTED,
                16,
            );
        }
        for control in (0..screen.slots)
            .map(UiControl::KilnSlot)
            .chain((0..36).map(UiControl::InventorySlot))
        {
            let Some(rect) = layout.rect(control) else {
                continue;
            };
            let (stack, selected) = match control {
                UiControl::KilnSlot(slot) => (
                    view.slots.get(slot as usize).and_then(Option::as_ref),
                    frame.kiln_source == Some(slot),
                ),
                UiControl::InventorySlot(slot) => (
                    frame.inventory[slot as usize].as_ref(),
                    frame.inventory_source == Some(slot),
                ),
                _ => unreachable!(),
            };
            self.rounded_panel(
                rect,
                [0.055, 0.075, 0.073, 0.95],
                if selected {
                    GOLD
                } else if frame.hovered == Some(control) {
                    TEXT
                } else {
                    EDGE
                },
                self.scale,
            );
            if let Some(stack) = stack {
                self.draw_stack_swatch(
                    inset(rect, rect.width * 0.23, rect.height * 0.23),
                    stack,
                    catalog,
                );
                let count = stack.count.to_string();
                let font_scale = 0.45f32.min(
                    (rect.width - 4.0 * self.scale) / (18.0 * self.scale * count.len() as f32),
                );
                self.text(
                    &count,
                    rect.x + 2.0 * self.scale,
                    rect.y + rect.height - (16.0 * font_scale + 1.0) * self.scale,
                    font_scale,
                    TEXT,
                    3,
                );
            }
        }
        let hovered = match frame.hovered {
            Some(UiControl::KilnSlot(slot)) => {
                view.slots.get(slot as usize).and_then(Option::as_ref)
            }
            Some(UiControl::InventorySlot(slot)) => {
                frame.inventory.get(slot as usize).and_then(Option::as_ref)
            }
            _ => None,
        };
        let label = hovered.map(|s| format!("{} X{}", item_name_for(s.item, catalog), s.count));
        self.text(
            &screen.hint,
            panel.x + 18.0 * self.scale,
            panel.y + panel.height - 33.0 * self.scale,
            0.4,
            MUTED,
            ((panel.width - 36.0 * self.scale) / (18.0 * self.scale * 0.4)) as usize,
        );
        self.text(
            frame
                .status
                .or(label.as_deref())
                .unwrap_or("SOURCE THEN DEST / RIGHT: ONE / E: CLOSE"),
            panel.x + 18.0 * self.scale,
            panel.y + panel.height - 18.0 * self.scale,
            0.45,
            MUTED,
            58,
        );
    }
}
