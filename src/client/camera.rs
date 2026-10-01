//! Keep presentation viewpoint separate from the authoritative eye/interaction origin.
use super::*;
use crate::render::camera::Perspective;

impl ClientApp {
    pub(super) fn view_camera(&self) -> Camera {
        self.perspective.view(self.camera(), |[x, y, z]| {
            self.block_at(x, y, z).is_none_or(|block| {
                self.catalog
                    .block(block)
                    .is_none_or(|definition| definition.solid)
            })
        })
    }

    pub(super) fn show_local_avatar(&self, view: Camera) -> bool {
        self.perspective != Perspective::FirstPerson
            && view.position.distance_squared(self.camera().position) >= 0.6 * 0.6
    }

    pub(super) fn prepare_local_avatar(&self, avatars: &mut [crate::render::VisualAvatar]) {
        if let Some(avatar) = avatars
            .iter_mut()
            .find(|a| Some(a.id) == self.owned_entity_id)
        {
            avatar.position = self.position;
            avatar.pose[0] = std::f32::consts::FRAC_PI_2 - self.yaw;
        }
    }

    pub(super) fn cycle_perspective(&mut self) {
        self.perspective = self.perspective.next();
        self.show_status(match self.perspective {
            Perspective::FirstPerson => "Camera: first person",
            Perspective::Behind => "Camera: third person (rear)",
            Perspective::Front => "Camera: third person (front)",
        });
    }
}

#[cfg(test)]
mod tests;
