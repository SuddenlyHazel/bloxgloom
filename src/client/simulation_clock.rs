//! Presentation phase for server-issued baked clips. This clock grants no
//! gameplay authority and stops extrapolating after a short delivery gap.
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
#[derive(Default)]
pub(super) struct Clock {
    sample: Option<(u64, Instant)>,
    presented: Cell<u64>,
}
impl Clock {
    pub(super) fn synchronize(&mut self, tick: u64) {
        self.synchronize_at(tick, Instant::now());
    }
    fn synchronize_at(&mut self, tick: u64, now: Instant) {
        if self.sample.is_none_or(|(old, _)| tick > old) {
            self.sample = Some((tick, now));
        }
    }
    pub(super) fn now(&self) -> u64 {
        self.now_at(Instant::now())
    }
    fn now_at(&self, now: Instant) -> u64 {
        let estimate = self.sample.map_or(0, |(tick, at)| {
            tick.saturating_add(
                (now.saturating_duration_since(at)
                    .min(Duration::from_millis(500))
                    .as_millis()
                    / 20) as u64,
            )
        });
        let tick = estimate.max(self.presented.get());
        self.presented.set(tick);
        tick
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation_clock_is_monotonic_bounded_and_ignores_old_or_duplicate_samples() {
        let at = Instant::now();
        let mut clock = Clock::default();
        assert_eq!(clock.now_at(at), 0);
        clock.synchronize_at(100, at);
        assert_eq!(clock.now_at(at + Duration::from_millis(110)), 105);
        clock.synchronize_at(99, at + Duration::from_millis(110));
        clock.synchronize_at(100, at + Duration::from_millis(110));
        assert_eq!(clock.now_at(at + Duration::from_secs(3)), 125);
        clock.synchronize_at(101, at + Duration::from_secs(3));
        assert_eq!(clock.now_at(at + Duration::from_secs(3)), 125);
        clock.synchronize_at(150, at + Duration::from_secs(4));
        assert_eq!(clock.now_at(at + Duration::from_secs(4)), 150);
    }
}
