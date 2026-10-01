//! Context interaction takes priority; generic item menus never swallow block placement.
use super::*;

impl ClientApp {
    pub(super) fn selected_item_is_placeable(&self) -> bool {
        self.inventory.slots[self.config.selected_slot]
            .as_ref()
            .and_then(|stack| self.catalog.item(stack.item))
            .is_some_and(|item| item.placeable.is_some())
    }

    pub(super) fn place_or_interact(&mut self) {
        if self.shift_down {
            self.edit_aimed_block(true);
            return;
        }
        if self.interact_aimed_mobile() || self.open_aimed_kiln() {
            return;
        }
        if self.selected_item_is_placeable() || !self.open_item_actions() {
            self.edit_aimed_block(true);
        }
    }
}

#[cfg(test)]
mod tests;
