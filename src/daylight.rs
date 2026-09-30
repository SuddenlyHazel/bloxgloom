//! Shared world-clock units. The server supplies time; clients only interpolate it.
pub(crate) const CYCLE_MS: u64 = 20 * 60 * 1_000;
/// Begin halfway through the daylight arc, rather than at sunrise.
pub(crate) const INITIAL_MS: u64 = CYCLE_MS / 4;

pub(crate) fn phase(elapsed_ms: u64) -> f32 {
    (elapsed_ms % CYCLE_MS) as f32 / CYCLE_MS as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_wraps_without_losing_precision_to_long_running_time() {
        assert_eq!(phase(INITIAL_MS), 0.25);
        assert_eq!(phase(INITIAL_MS + CYCLE_MS), 0.25);
        let time = (u64::MAX / CYCLE_MS - 1) * CYCLE_MS + INITIAL_MS;
        assert_eq!(phase(time), 0.25);
    }
}
