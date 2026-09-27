use super::*;

impl UiBuilder<'_> {
    pub(super) fn draw_chest(&mut self, frame: &UiFrame<'_>, layout: &UiLayout, catalog: &Catalog) {
        self.screen_dim();
        let panel = layout.kiln_panel();
        self.panel(panel);
        self.text(
            "CHEST  /  27 SLOTS",
            panel.x + 20.0 * self.scale,
            panel.y + 14.0 * self.scale,
            0.8,
            TEXT,
            28,
        );
        let Some(chest) = &frame.kiln else {
            return;
        };
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
        for control in (0..27)
            .map(UiControl::KilnSlot)
            .chain((0..36).map(UiControl::InventorySlot))
        {
            let Some(rect) = layout.rect(control) else {
                continue;
            };
            let (stack, selected) = match control {
                UiControl::KilnSlot(slot) => (
                    chest.slots.get(slot as usize).and_then(Option::as_ref),
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
                self.draw_item_swatch(
                    inset(rect, rect.width * 0.23, rect.height * 0.23),
                    stack.item,
                    catalog,
                );
                self.text(
                    &stack.count.to_string(),
                    rect.x + 2.0 * self.scale,
                    rect.y + rect.height - 10.0 * self.scale,
                    0.45,
                    TEXT,
                    3,
                );
            }
        }
        let hovered = match frame.hovered {
            Some(UiControl::KilnSlot(slot)) => {
                chest.slots.get(slot as usize).and_then(Option::as_ref)
            }
            Some(UiControl::InventorySlot(slot)) => {
                frame.inventory.get(slot as usize).and_then(Option::as_ref)
            }
            _ => None,
        };
        let label = hovered.map(|s| format!("{} X{}", item_name_for(s.item, catalog), s.count));
        self.text(
            frame
                .status
                .or(label.as_deref())
                .unwrap_or("SOURCE THEN DEST / RIGHT: ONE / E: CLOSE"),
            panel.x + 18.0 * self.scale,
            panel.y + panel.height - 20.0 * self.scale,
            0.5,
            MUTED,
            58,
        );
    }
}
