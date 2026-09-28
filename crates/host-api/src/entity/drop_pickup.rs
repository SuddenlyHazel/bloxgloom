//! Authoritative pickup eligibility policy shared by automatic selection and
//! exact inventory extraction. Ownership still changes only in the host WAL.

#[derive(Clone, Copy, Debug)]
pub struct DropLifetime {
    pub created_ms: u64,
    pub now_ms: u64,
    pub lifetime_ms: u64,
}

impl DropLifetime {
    /// Backward clock movement does not expire a drop. At the exact lifetime
    /// boundary it is no longer mergeable or pickable and may be despawned.
    pub fn expired(self) -> bool {
        self.now_ms.saturating_sub(self.created_ms) >= self.lifetime_ms
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DropPickupContext {
    pub age_ms: u64,
    pub delay_ms: u64,
    pub lifetime_ms: u64,
}

impl DropPickupContext {
    /// An item can be extracted only after its delay and before expiration.
    pub fn extractable(self) -> bool {
        self.age_ms >= self.delay_ms && self.age_ms < self.lifetime_ms
    }

    /// Automatic pickup additionally requires a captured in-range position.
    pub fn in_range(self, player: [f32; 3], drop: [f32; 3], range_sq: f32) -> bool {
        self.extractable()
            && range_sq.is_finite()
            && range_sq >= 0.0
            && player
                .iter()
                .chain(drop.iter())
                .all(|value| value.is_finite())
            && player
                .iter()
                .zip(drop)
                .map(|(from, to)| (from - to).powi(2))
                .sum::<f32>()
                <= range_sq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickup_delay_range_and_expiry_have_exclusive_boundaries() {
        let mut context = DropPickupContext {
            age_ms: 249,
            delay_ms: 250,
            lifetime_ms: 3000,
        };
        assert!(!context.extractable());
        context.age_ms = 250;
        assert!(context.extractable());
        assert!(context.in_range([0.0; 3], [1.0, 0.0, 0.0], 1.0));
        assert!(!context.in_range([0.0; 3], [1.001, 0.0, 0.0], 1.0));
        context.age_ms = 3000;
        assert!(!context.extractable());
        assert!(
            !DropLifetime {
                created_ms: 100,
                now_ms: 99,
                lifetime_ms: 3000
            }
            .expired()
        );
        assert!(
            DropLifetime {
                created_ms: 100,
                now_ms: 3100,
                lifetime_ms: 3000
            }
            .expired()
        );
    }
}
