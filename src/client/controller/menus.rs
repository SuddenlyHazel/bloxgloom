//! A virtual pointer lets controllers operate existing menus, sliders and slots.
use super::*;
impl ClientApp {
    pub(super) fn controller_menu(&mut self, sample: Snapshot, dt: f32) {
        let movement = (deadzone(sample.movement) + sample.dpad).clamp_length_max(1.0);
        let scroll = deadzone(sample.look) * (dt.clamp(0.0, 0.05) * 600.0);
        let held = self.controller.input.held;
        let desired = if held & SOUTH != 0 {
            Some(false)
        } else if held & WEST != 0 {
            Some(true)
        } else {
            None
        };
        let changed = desired != self.controller.pointer_down;
        if movement == Vec2::ZERO && scroll == Vec2::ZERO && !changed {
            return;
        }
        let size = self.window.as_ref().map_or(Vec2::new(1280.0, 720.0), |w| {
            let size = w.inner_size();
            Vec2::new(size.width as f32, size.height as f32)
        });
        let pointer = self.controller.pointer.get_or_insert(size * 0.5);
        *pointer = (*pointer + Vec2::new(movement.x, -movement.y) * dt.clamp(0.0, 0.05) * 900.0)
            .clamp(Vec2::ZERO, (size - Vec2::ONE).max(Vec2::ZERO));
        let position = *pointer;
        self.cursor = (position.x, position.y);
        self.controller_pointer(Some(position), None, scroll);
        if changed {
            if let Some(secondary) = self.controller.pointer_down {
                self.controller_pointer(None, Some((false, secondary)), Vec2::ZERO);
            }
            if let Some(secondary) = desired {
                self.controller_pointer(None, Some((true, secondary)), Vec2::ZERO);
            }
        }
        self.controller.pointer_down = desired;
    }

    pub(super) fn controller_pointer(
        &mut self,
        position: Option<Vec2>,
        button: Option<(bool, bool)>,
        scroll: Vec2,
    ) {
        if let Some(renderer) = &mut self.renderer {
            renderer.game_ui_controller_pointer(
                position.map(|p| p.to_array()),
                button,
                scroll.to_array(),
            );
        }
    }
}
