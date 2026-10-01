//! Ephemeral active-step eligibility; recovery and paused bodies never catch up.
use super::super::EntityId;
#[derive(Default)]
pub(in crate::server) struct Cadence(std::collections::BTreeSet<EntityId>);
impl Cadence {
    pub(super) fn steps(&self, id: EntityId, elapsed: u64) -> u32 {
        if self.0.contains(&id) && elapsed >= super::STEP_TICKS * 2 {
            2
        } else {
            1
        }
    }
    pub(super) fn active(&mut self, id: EntityId) {
        if !self.0.contains(&id) && self.0.len() >= super::MAX_BODIES {
            self.0.pop_first();
        }
        self.0.insert(id);
    }
    pub(super) fn pause(&mut self, id: EntityId) {
        self.0.remove(&id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_catchup_is_bounded_and_cold_or_paused_bodies_take_one_step() {
        let id = EntityId::new(1).unwrap();
        let mut cadence = Cadence::default();
        assert_eq!(cadence.steps(id, 1000), 1);
        cadence.active(id);
        assert_eq!(cadence.steps(id, 2), 1);
        assert_eq!(cadence.steps(id, 4), 2);
        assert_eq!(cadence.steps(id, 1000), 2);
        cadence.pause(id);
        assert_eq!(cadence.steps(id, 1000), 1);
        for value in 1..=super::super::MAX_BODIES as u64 + 1 {
            cadence.active(EntityId::new(value).unwrap());
        }
        assert_eq!(cadence.0.len(), super::super::MAX_BODIES);
    }
}
