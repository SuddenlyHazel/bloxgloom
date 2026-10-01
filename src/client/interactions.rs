//! Context interaction takes priority; generic item menus never swallow block placement.
use super::*;

impl ClientApp {
    pub(super) fn break_button(&mut self, pressed: bool, now: Instant) {
        if !pressed {
            self.next_break = None;
        } else if self.screen == UiScreen::Playing
            && self.grabbed
            && !self.disconnected
            && self.next_break.is_none()
        {
            self.next_break = Some(now);
            self.repeat_held_break(now);
        }
    }

    pub(super) fn repeat_held_break(&mut self, now: Instant) {
        if self.screen != UiScreen::Playing || !self.grabbed || self.disconnected {
            self.next_break = None;
            return;
        }
        if self.next_break.is_some_and(|deadline| now >= deadline) {
            // One action per authored swing. A delayed frame never catches up
            // with a burst, and every repeat raycasts the current authoritative chunks.
            self.next_break =
                Some(now + Duration::from_secs_f32(crate::render::character_tool_duration(true)));
            self.edit_aimed_block_at(false, now);
        }
    }

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
