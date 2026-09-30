//! Session-local interpolation of the server's world clock.
use crate::daylight::{CYCLE_MS, INITIAL_MS};
use std::time::Instant;

pub(super) struct Clock {
    time: u64,
    received: Instant,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            time: INITIAL_MS,
            received: Instant::now(),
        }
    }
}

impl Clock {
    pub(super) fn synchronize(&mut self, time: u64) {
        self.time = time;
        self.received = Instant::now();
    }

    pub(super) fn now(&self) -> u64 {
        (self.time + (self.received.elapsed().as_millis() % u128::from(CYCLE_MS)) as u64) % CYCLE_MS
    }
}
