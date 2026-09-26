use super::*;

impl UiBuilder<'_> {
    pub(super) fn draw_kiln(&mut self, frame: &UiFrame<'_>, layout: &UiLayout, catalog: &Catalog) {
        self.screen_dim();
        let panel = layout.kiln_panel();
        self.panel(panel);
        self.text(
            "KILN",
            panel.x + 20.0 * self.scale,
            panel.y + 14.0 * self.scale,
            1.0,
            TEXT,
            10,
        );
        let Some(kiln) = &frame.kiln else {
            return;
        };
        let state = if kiln.lit { "BURNING" } else { "UNLIT" };
        self.text(
            &format!("{state}  /  FUEL {:.1}S", f32::from(kiln.fuel) * 0.4),
            panel.x + 150.0 * self.scale,
            panel.y + 18.0 * self.scale,
            0.55,
            GOLD,
            32,
        );
        for slot in 0..3u8 {
            let Some(rect) = layout.rect(UiControl::KilnSlot(slot)) else {
                continue;
            };
            self.text(
                ["FUEL", "INPUT", "OUTPUT"][slot as usize],
                rect.x,
                rect.y - 15.0 * self.scale,
                0.5,
                MUTED,
                8,
            );
            self.rounded_panel(
                rect,
                [0.04, 0.05, 0.055, 1.0],
                if frame.kiln_source == Some(slot) {
                    GOLD
                } else {
                    EDGE
                },
                2.0 * self.scale,
            );
            if let Some(stack) = &kiln.slots[slot as usize] {
                self.draw_item_swatch(
                    inset(rect, rect.width * 0.22, rect.height * 0.22),
                    stack.item,
                    catalog,
                );
                self.text(
                    &stack.count.to_string(),
                    rect.x + 3.0 * self.scale,
                    rect.y + rect.height - 13.0 * self.scale,
                    0.55,
                    TEXT,
                    3,
                );
            }
        }
        if let (Some(input), Some(output)) = (
            layout.rect(UiControl::KilnSlot(1)),
            layout.rect(UiControl::KilnSlot(2)),
        ) {
            let x = input.x + input.width + 12.0 * self.scale;
            let width = output.x - x - 16.0 * self.scale;
            self.rect(
                x,
                input.y + 12.0 * self.scale,
                width,
                10.0 * self.scale,
                [0.08, 0.09, 0.08, 1.0],
            );
            self.rect(
                x,
                input.y + 12.0 * self.scale,
                width * f32::from(kiln.progress) / 255.0,
                10.0 * self.scale,
                GOLD,
            );
        }
        self.text(
            "GRAVEL > STONE  /  FUEL: WOOD, STICKS, SAPLINGS",
            panel.x + 20.0 * self.scale,
            panel.y + 121.0 * self.scale,
            0.5,
            MUTED,
            52,
        );
        for slot in 0..36u8 {
            let Some(rect) = layout.rect(UiControl::InventorySlot(slot)) else {
                continue;
            };
            self.rounded_panel(
                rect,
                [0.055, 0.075, 0.073, 0.95],
                if frame.inventory_source == Some(slot) {
                    GOLD
                } else if frame.hovered == Some(UiControl::InventorySlot(slot)) {
                    TEXT
                } else {
                    EDGE
                },
                self.scale,
            );
            if let Some(stack) = &frame.inventory[slot as usize] {
                self.draw_item_swatch(
                    inset(rect, rect.width * 0.23, rect.height * 0.23),
                    stack.item,
                    catalog,
                );
                self.text(
                    &stack.count.to_string(),
                    rect.x + 3.0 * self.scale,
                    rect.y + rect.height - 12.0 * self.scale,
                    0.5,
                    TEXT,
                    3,
                );
            }
        }
        let hovered = match frame.hovered {
            Some(UiControl::KilnSlot(slot)) => {
                kiln.slots.get(slot as usize).and_then(Option::as_ref)
            }
            Some(UiControl::InventorySlot(slot)) => {
                frame.inventory.get(slot as usize).and_then(Option::as_ref)
            }
            _ => None,
        };
        let hover_label = hovered
            .map(|stack| format!("{}  X{}", item_name_for(stack.item, catalog), stack.count));
        let footer = frame
            .status
            .or(hover_label.as_deref())
            .unwrap_or("SOURCE THEN DEST  /  RIGHT: ONE  /  E: CLOSE");
        self.text(
            footer,
            panel.x + 18.0 * self.scale,
            panel.y + panel.height - 20.0 * self.scale,
            0.5,
            MUTED,
            58,
        );
    }
}
