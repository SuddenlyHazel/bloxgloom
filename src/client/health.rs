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
        if self.pending_actions.len() >= MAX_OUTSTANDING_ACTIONS {
            return;
        }
        let Some(action_id) = self.actions.allocate() else {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_health_death_resets_input_blocks_gameplay_and_retirement_clears_state() {
        let (mut network, outgoing) = Network::capture_outgoing_for_test();
        network.profile = 1;
        let mut app = ClientApp::new(
            network,
            Config::default(),
            std::env::temp_dir().join("unused-health-config"),
        );
        app.accept(ServerMessage::ActionSession {
            epoch: 7,
            next_seq: 1,
            acked_seq: 0,
        });
        app.keys.forward = true;
        app.unacked.push_back((1, Vec3::X));
        let dead = bloxgloom_host_api::player_health::View::new(
            bloxgloom_host_api::player_health::State {
                current: 0,
                life: 2,
                ..Default::default()
            },
            1,
        );
        app.accept(ServerMessage::PlayerHealth {
            profile: 1,
            session: 7,
            health: dead,
        });
        assert_eq!(app.screen, UiScreen::Dead);
        app.set_screen(UiScreen::Playing);
        assert_eq!(
            app.screen,
            UiScreen::Dead,
            "a stale menu intent hid manual respawn"
        );
        assert!(!app.keys.forward);
        assert!(app.unacked.is_empty());
        app.queue_command(ClientMessage::Move {
            seq: 2,
            dx: 1.,
            dy: 0.,
            dz: 0.,
        });
        assert!(outgoing.try_recv().is_err());
        app.accept(ServerMessage::PlayerHealth {
            profile: 1,
            session: 7,
            health: bloxgloom_host_api::player_health::View::new(Default::default(), 0),
        });
        assert!(!app.health.alive);
        let alive = bloxgloom_host_api::player_health::View::new(
            bloxgloom_host_api::player_health::State {
                life: 3,
                ..Default::default()
            },
            2,
        );
        app.accept(ServerMessage::PlayerHealth {
            profile: 1,
            session: 7,
            health: alive,
        });
        assert_eq!(app.screen, UiScreen::Playing);
        app.queue_command(ClientMessage::Move {
            seq: 2,
            dx: 1.,
            dy: 0.,
            dz: 0.,
        });
        assert!(matches!(
            outgoing.try_recv().unwrap(),
            ClientMessage::PlayerIntent { life: 3, .. }
        ));
        app.retire_session();
        assert_eq!(
            app.health,
            bloxgloom_host_api::player_health::View::new(Default::default(), 0)
        );
    }
}
