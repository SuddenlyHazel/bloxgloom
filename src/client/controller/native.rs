//! Gilrs supplies platform input, hotplug, and the bundled SDL controller mappings.
use super::state::*;
use gilrs::{Axis, Button, GamepadId, Gilrs, GilrsBuilder};
use glam::Vec2;

pub(super) struct Native {
    gilrs: Gilrs,
    active: Option<GamepadId>,
}
impl Native {
    pub fn new() -> Option<Self> {
        match GilrsBuilder::new().with_force_feedback(false).build() {
            Ok(gilrs) => Some(Self {
                gilrs,
                active: None,
            }),
            Err(error) => {
                tracing::warn!(%error, "Controller input unavailable");
                None
            }
        }
    }
    pub fn poll(&mut self) -> Snapshot {
        // Bound event work during noisy USB input; next_event maintains cached state.
        for _ in 0..256 {
            if self.gilrs.next_event().is_none() {
                break;
            }
        }
        if self
            .active
            .is_some_and(|id| !self.gilrs.gamepad(id).is_connected())
        {
            tracing::info!("Controller disconnected");
            self.active = None;
            return Snapshot {
                unavailable: true,
                ..Default::default()
            };
        }
        if self.active.is_none() {
            self.active = self.gilrs.gamepads().find(|(_, pad)| {
                pad.button_code(Button::South).is_some()
                    && pad.axis_code(Axis::LeftStickX).is_some()
                    && pad.axis_code(Axis::LeftStickY).is_some()
            }).map(|(id, pad)| {
                tracing::info!(name = pad.name(), mapping = ?pad.mapping_source(), "Controller connected");
                id
            });
        }
        let Some(id) = self.active else {
            return Snapshot {
                unavailable: true,
                ..Default::default()
            };
        };
        let pad = self.gilrs.gamepad(id);
        let bindings = [
            (Button::South, SOUTH),
            (Button::East, EAST),
            (Button::West, WEST),
            (Button::North, NORTH),
            (Button::Start, START),
            (Button::Select, SELECT),
            (Button::LeftTrigger, LB),
            (Button::RightTrigger, RB),
            (Button::LeftTrigger2, LT),
            (Button::RightTrigger2, RT),
            (Button::LeftThumb, L3),
        ];
        let mut buttons = 0;
        for (button, flag) in bindings {
            if pad.is_pressed(button) {
                buttons |= flag;
            }
        }
        let down = |button| u8::from(pad.is_pressed(button)) as f32;
        Snapshot {
            movement: Vec2::new(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)),
            look: Vec2::new(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
            dpad: Vec2::new(
                down(Button::DPadRight) - down(Button::DPadLeft),
                down(Button::DPadUp) - down(Button::DPadDown),
            ),
            buttons,
            unavailable: false,
        }
    }
}
