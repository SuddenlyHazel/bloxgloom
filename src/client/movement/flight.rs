//! Only an authoritative server reply changes the local movement mode.
use crate::client::*;

#[cfg(test)]
pub(crate) mod tests;

pub(in crate::client) struct Flight {
    pub flying: bool,
    pub pending: Option<bool>,
}
impl Default for Flight {
    fn default() -> Self {
        Self {
            flying: true,
            pending: None,
        }
    }
}
impl ClientApp {
    pub(in crate::client) fn toggle_flying(&mut self) {
        if !self.admin_enabled || self.flight.pending.is_some() {
            return;
        }
        self.cancel_sprint();
        let flying = !self.flight.flying;
        if self.network.send(ClientMessage::SetFlying { flying }) {
            self.flight.pending = Some(flying);
        }
    }
    pub(in crate::client) fn accept_flying(&mut self, flying: bool) {
        let requested = self.flight.pending.take();
        self.cancel_sprint();
        self.flight.flying = flying;
        if !flying {
            for (_, delta) in &mut self.unacked {
                delta.y = 0.0;
            }
        }
        self.show_status(if requested.is_some_and(|value| value != flying) {
            "Flying mode requires admin access"
        } else if flying {
            "Flying enabled — Space rises, Ctrl descends"
        } else {
            "Walking enabled — Space jumps, Shift crouches"
        });
    }
    pub(in crate::client) fn jump(&mut self) {
        if !self.flight.flying
            && self.flight.pending.is_none()
            && self.grabbed
            && self.screen == UiScreen::Playing
        {
            self.network.send(ClientMessage::Jump);
        }
    }
}
