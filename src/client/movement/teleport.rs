//! Install an authoritative teleport before acknowledging the movement barrier.
use crate::client::*;
impl ClientApp {
    pub(in crate::client) fn accept_teleport(
        &mut self,
        profile: u128,
        session: u64,
        reset: u64,
        position: [f32; 3],
    ) {
        if profile != self.network.profile
            || session != self.actions.epoch
            || session == 0
            || reset == 0
        {
            self.fail_session("Player teleport has wrong session identity");
        } else if reset > self.movement_reset {
            self.movement_reset = reset;
            self.unacked.clear();
            self.lod.reset();
            if let Some(renderer) = &mut self.renderer {
                renderer.clear_lod();
            }
            self.position = Vec3::from_array(position);
            if !self.network.send(ClientMessage::MovementReady {
                session,
                reset,
                next_seq: self.next_seq,
            }) {
                self.fail_session("Movement reset queue unavailable");
            }
        }
    }
}
