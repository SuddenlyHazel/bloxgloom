//! Health and death presentation follows exact-session server snapshots.
use super::*;
impl ClientApp {
    pub(super) fn health_intent_message(&self, message: ClientMessage) -> ClientMessage {
        if self.health.life == 1 {
            message
        } else {
            ClientMessage::PlayerIntent {
                life: self.health.life,
                message: Box::new(message),
            }
        }
    }

    pub(super) fn accept_health(
        &mut self,
        profile: u128,
        session: u64,
        health: bloxgloom_host_api::player_health::View,
    ) {
        if profile != self.network.profile
            || session == 0
            || (self.actions.epoch != 0 && session != self.actions.epoch)
        {
            self.fail_session("Player health has wrong session identity");
            return;
        }
        if health.revision < self.health.revision || health.life < self.health.life {
            return;
        }
        self.health = health;
        if !health.alive {
            self.cancel_sprint();
            self.keys = Default::default();
            self.unacked.clear();
            self.set_screen(UiScreen::Dead);
        } else if self.screen == UiScreen::Dead {
            self.set_screen(UiScreen::Playing);
        }
    }
    pub(super) fn request_respawn(&mut self) {
        if self.health.alive || self.disconnected {
            return;
        }
        let Some(action_id) = self.allocate_action_id() else {
            return;
        };
        let request = bloxgloom_host_api::actions::Request {
            key: crate::gameplay::respawn::KEY.into(),
            version: 1,
            slot: 0,
            inventory_revision: self.inventory.revision,
            entity: 0,
            entity_revision: 0,
            arguments: self.health.revision.to_le_bytes().to_vec(),
        };
        let Some(payload) = request.encode() else {
            return;
        };
        self.queue_command(ClientMessage::EntityInteract {
            action_id,
            target: [0; 3],
            payload,
        });
    }
}
