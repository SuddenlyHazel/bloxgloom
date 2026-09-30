//! Headless real-client operations for the combined responsiveness regression.
use super::*;

impl PackageActionProbe {
    pub(crate) fn mixed_command(&mut self, text: &str) -> ClientMessage {
        let id = action_id(self.app.actions.epoch, self.app.actions.next_seq);
        self.app.admin_input = text.into();
        self.app.admin_run();
        self.app.pending_actions[&id].clone()
    }
    pub(crate) fn mixed_drain(&mut self) {
        while let Ok(incoming) = self.app.network.incoming.try_recv() {
            match incoming {
                Incoming::Message(message) => self.app.accept(*message),
                Incoming::Closed(reason) => panic!("mixed client closed: {reason}"),
            }
            assert!(!self.app.disconnected);
        }
    }

    pub(crate) fn mixed_wait_slot_count(&mut self, slot: usize, count: u16) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.app.inventory.slots[slot]
            .as_ref()
            .map_or(0, |stack| stack.count)
            != count
        {
            let message = self.read(deadline);
            self.app.accept(message);
            assert!(!self.app.disconnected);
        }
    }

    pub(crate) fn mixed_move(&mut self, sequence: u64, dx: f32) -> Duration {
        let start = Instant::now();
        assert!(self.app.network.send(ClientMessage::Move {
            seq: sequence,
            dx,
            dy: 0.0,
            dz: 0.0,
        }));
        let deadline = start + Duration::from_secs(10);
        loop {
            let message = self.read(deadline);
            let acknowledged =
                matches!(message, ServerMessage::Position { ack_seq, .. } if ack_seq == sequence);
            self.app.accept(message);
            assert!(!self.app.disconnected);
            if acknowledged {
                return start.elapsed();
            }
        }
    }

    pub(crate) fn mixed_transfer(&mut self, from: u8, to: u8) -> ClientMessage {
        let message = ClientMessage::InventoryMove {
            action_id: self.app.allocate_action_id().unwrap(),
            from,
            to,
            count: 1,
        };
        self.app.queue_command(message.clone());
        message
    }

    pub(crate) fn mixed_wait_block(&mut self, at: [i32; 3], block: BlockId) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.app.block_at(at[0], at[1], at[2]) != Some(block) {
            let message = self.read(deadline);
            self.app.accept(message);
            assert!(!self.app.disconnected);
        }
    }
}
