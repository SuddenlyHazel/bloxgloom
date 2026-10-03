//! Double-tap forward intent; speed and public animation follow server approval.
use crate::client::*;

const DOUBLE_TAP: Duration = Duration::from_millis(300);

#[derive(Default)]
pub(in crate::client) struct Sprint {
    last_press: Option<Instant>,
    requested: bool,
    pending: VecDeque<bool>,
}

impl ClientApp {
    pub(in crate::client) fn backward_input(&mut self, pressed: bool) {
        self.keys.back = pressed;
        if pressed {
            self.cancel_sprint();
        }
    }

    pub(in crate::client) fn forward_input(&mut self, pressed: bool, repeat: bool, now: Instant) {
        if repeat || pressed == self.keys.forward {
            return;
        }
        self.keys.forward = pressed;
        if !pressed {
            self.request_sprint(false);
            return;
        }
        if !self.can_sprint() {
            self.cancel_sprint();
            return;
        }
        let double = self
            .sprint
            .last_press
            .is_some_and(|last| now.saturating_duration_since(last) <= DOUBLE_TAP);
        self.sprint.last_press = Some(now);
        if double {
            self.request_sprint(true);
        }
    }

    fn can_sprint(&self) -> bool {
        self.screen == UiScreen::Playing
            && self.grabbed
            && !self.flight.flying
            && self.flight.pending.is_none()
            && !self.crouch_requested
            && !self.crouching()
            && !self.keys.back
    }

    pub(in crate::client) fn sprinting(&self) -> bool {
        self.sprint.requested
            && self.keys.forward
            && self.can_sprint()
            && self
                .owned_entity_id
                .is_some_and(|id| self.player_sprints.contains_key(&id))
    }

    fn request_sprint(&mut self, sprinting: bool) {
        if sprinting == self.sprint.requested {
            return;
        }
        // Keep control requests ordered before movement on the same connection.
        if self.sprint.pending.len() >= 16
            || !self.network.send(ClientMessage::SetSprinting { sprinting })
        {
            self.fail_session("Sprint control queue unavailable");
            return;
        }
        self.sprint.requested = sprinting;
        self.sprint.pending.push_back(sprinting);
    }

    pub(in crate::client) fn cancel_sprint(&mut self) {
        self.sprint.last_press = None;
        self.request_sprint(false);
    }

    pub(in crate::client) fn accept_sprint(&mut self, entity_id: u64, sprinting: bool) {
        let own = self.owned_entity_id == Some(entity_id);
        let expected = if own {
            self.sprint.pending.pop_front()
        } else {
            None
        };
        if sprinting {
            if self.player_sprints.len() < 256 || self.player_sprints.contains_key(&entity_id) {
                self.player_sprints.insert(entity_id, true);
            } else {
                self.fail_session("Player sprint list exceeds admission bound");
            }
        } else {
            self.player_sprints.remove(&entity_id);
            if own && self.sprint.pending.is_empty() && expected != Some(false) {
                // Denial or an unsolicited server stop requires a fresh double
                // tap. An earlier release acknowledgement cannot cancel a newer tap.
                self.sprint.requested = false;
                self.sprint.last_press = None;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
