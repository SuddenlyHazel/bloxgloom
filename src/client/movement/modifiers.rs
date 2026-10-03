//! Install runtime movement rates before acknowledging their prediction fence.
use crate::client::*;
impl ClientApp {
    pub(in crate::client) fn accept_modifiers(
        &mut self,
        profile: u128,
        session: u64,
        reset: u64,
        position: [f32; 3],
        movement: bloxgloom_host_api::player_modifiers::Movement,
    ) {
        if profile != self.network.profile
            || session != self.actions.epoch
            || session == 0
            || reset == 0
            || movement.validate().is_err()
        {
            self.fail_session("Player modifiers have wrong session identity");
        } else if reset > self.movement_reset {
            self.movement_reset = reset;
            self.movement_modifiers = movement;
            self.unacked.clear();
            self.position = Vec3::from_array(position);
            if !self.network.send(ClientMessage::MovementReady {
                session,
                reset,
                next_seq: self.next_seq,
            }) {
                self.fail_session("Movement modifier reset queue unavailable");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_modifier_prediction_fence_keeps_held_input_and_ignores_stale_configuration() {
        let (mut network, outgoing) = Network::capture_outgoing_for_test();
        network.profile = 1;
        let mut app = ClientApp::new(
            network,
            Config::default(),
            std::env::temp_dir().join("unused-player-modifier-config"),
        );
        app.accept(ServerMessage::ActionSession {
            epoch: 5,
            next_seq: 1,
            acked_seq: 0,
        });
        app.next_seq = 9;
        app.keys.forward = true;
        app.unacked.push_back((8, Vec3::X));
        let slow = bloxgloom_host_api::player_modifiers::Movement {
            speed: 0.5,
            ..Default::default()
        };
        app.accept(ServerMessage::PlayerModifiers {
            profile: 1,
            session: 5,
            reset: 1,
            position: [1.5, 4.0, 1.5],
            movement: slow,
        });
        assert!(app.unacked.is_empty());
        assert!(app.keys.forward);
        assert_eq!(app.movement_modifiers, slow);
        assert_eq!(
            outgoing.try_recv().unwrap(),
            ClientMessage::MovementReady {
                session: 5,
                reset: 1,
                next_seq: 9
            }
        );
        app.accept(ServerMessage::PlayerModifiers {
            profile: 1,
            session: 5,
            reset: 1,
            position: [0.0; 3],
            movement: Default::default(),
        });
        assert_eq!(app.position, Vec3::new(1.5, 4.0, 1.5));
        assert_eq!(app.movement_modifiers, slow);
        assert!(outgoing.try_recv().is_err());
        app.retire_session();
        assert_eq!(app.movement_modifiers, Default::default());
    }
}
