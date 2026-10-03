//! Controller intent uses the same authoritative gameplay requests as mouse/keyboard.
use super::*;
use glam::Vec2;
mod menus;
mod native;
mod state;
use state::*;

#[derive(Default)]
pub(super) struct Controller {
    native: Option<native::Native>,
    input: Input,
    pub movement: Vec2,
    pub breaking: bool,
    pub crouching: bool,
    pub rising: bool,
    pub descending: bool,
    mouse_breaking: bool,
    pointer_down: Option<bool>,
    pointer: Option<Vec2>,
}
impl Controller {
    pub fn initialize(&mut self) {
        self.native = native::Native::new();
    }
}

impl ClientApp {
    pub(super) fn clear_controller_input(&mut self) {
        self.controller.input.fence();
        self.controller.movement = Vec2::ZERO;
        self.controller.breaking = false;
        self.controller.mouse_breaking = false;
        self.controller.crouching = false;
        self.controller.rising = false;
        self.controller.descending = false;
        if let Some(secondary) = self.controller.pointer_down {
            // Cancel a drag outside every widget when changing screens/focus.
            self.controller_pointer(
                Some(Vec2::splat(-1000.0)),
                Some((false, secondary)),
                Vec2::ZERO,
            );
        }
        self.controller.pointer_down = None;
    }

    pub(super) fn mouse_break_button(&mut self, held: bool, now: Instant) {
        self.controller.mouse_breaking = held;
        self.break_button(held || self.controller.breaking, now);
    }

    pub(super) fn poll_controller(&mut self, dt: f32, now: Instant) {
        let Some(native) = &mut self.controller.native else {
            return;
        };
        let sample = native.poll();
        self.controller_sample(sample, dt, now);
    }

    fn controller_sample(&mut self, sample: Snapshot, dt: f32, now: Instant) {
        self.controller.input.update(sample);
        if sample.unavailable {
            let mouse_breaking = self.controller.mouse_breaking;
            self.clear_controller_input();
            self.controller.mouse_breaking = mouse_breaking;
            self.break_button(mouse_breaking, now);
            self.request_crouch(self.shift_down);
            if !self.keys.forward {
                self.cancel_sprint();
            }
            return;
        }
        if self.disconnected || self.window.as_ref().is_some_and(|w| !w.has_focus()) {
            self.clear_controller_input();
            self.break_button(false, now);
            self.cancel_sprint();
            self.request_crouch(false);
            return;
        }
        let pressed = self.controller.input.pressed;
        if pressed & START != 0 || (self.screen != UiScreen::Playing && pressed & EAST != 0) {
            self.on_escape();
            return;
        }
        if pressed & NORTH != 0 && matches!(self.screen, UiScreen::Playing | UiScreen::Inventory) {
            self.toggle_inventory();
            return;
        }
        if self.screen != UiScreen::Playing {
            self.controller_menu(sample, dt);
            return;
        }
        if !self.grabbed && !self.chat.open && pressed & SOUTH != 0 {
            self.set_grab(true);
            self.clear_controller_input();
            return;
        }
        if self.chat.open || !self.grabbed {
            self.clear_controller_input();
            self.break_button(false, now);
            self.cancel_sprint();
            self.request_crouch(false);
            return;
        }
        self.controller.movement = self.controller.input.movement;
        let held = self.controller.input.held;
        let look = look_delta(sample.look, dt);
        self.yaw += look.x;
        self.pitch = (self.pitch + look.y).clamp(-1.55, 1.55);
        self.controller.rising = held & SOUTH != 0;
        self.controller.descending = held & EAST != 0;
        self.controller.crouching = !self.flight.flying && held & EAST != 0;
        self.request_crouch(self.shift_down || self.controller.crouching);
        if pressed & SOUTH != 0 {
            self.jump();
        }
        if pressed & L3 != 0 {
            self.controller_sprint();
        }
        if !self.forward_held() || self.controller.movement.y < -0.1 {
            self.cancel_sprint();
        }
        if pressed & SELECT != 0 {
            self.cycle_perspective();
        }
        if pressed & LB != 0 {
            self.select_slot((self.config.selected_slot + HOTBAR_SLOTS - 1) % HOTBAR_SLOTS);
        }
        if pressed & RB != 0 {
            self.select_slot((self.config.selected_slot + 1) % HOTBAR_SLOTS);
        }
        if pressed & (LT | WEST) != 0 {
            self.place_or_interact();
        }
        if self.screen != UiScreen::Playing {
            return;
        }
        let breaking = held & RT != 0;
        if breaking != self.controller.breaking {
            self.controller.breaking = breaking;
            self.break_button(breaking || self.controller.mouse_breaking, now);
        }
    }
}

#[cfg(test)]
mod tests;
