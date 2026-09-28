//! Startup-frozen authoritative drop parameters, independent of presentation.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropPolicy {
    /// Blocks/second squared, 0..=96.
    pub gravity: f32,
    /// Blocks/second, 0..=60; bounds each fixed-step collision sweep.
    pub terminal_speed: f32,
    /// Collision half-width/height, 0.01..=0.49. Never changes rendered size.
    pub radius: f32,
    /// Authenticated actor distance in blocks, 0..=8.
    pub pickup_range: f32,
    /// Same-stack merge distance in blocks, 0..=8; zero disables merging.
    pub merge_range: f32,
    /// Wall-clock lifetime, 1_000..=86_400_000 milliseconds. Expiry is serviced
    /// by the ordinary one-second durable sweep, not a client animation clock.
    pub lifetime_ms: u64,
}

impl Default for DropPolicy {
    fn default() -> Self {
        Self {
            gravity: 24.0,
            terminal_speed: 30.0,
            radius: 0.18,
            pickup_range: 2.25,
            merge_range: 1.0,
            lifetime_ms: 600_000,
        }
    }
}

impl DropPolicy {
    pub const MAX_PICKUP_RANGE: f32 = 8.0;
    pub const BYTE_LEN: usize = 28;

    pub fn valid(self) -> bool {
        fn bounded(value: f32, min: f32, max: f32) -> bool {
            value.is_finite()
                && (min..=max).contains(&value)
                && !(value == 0.0 && value.is_sign_negative())
        }
        bounded(self.gravity, 0.0, 96.0)
            && bounded(self.terminal_speed, 0.0, 60.0)
            && bounded(self.radius, 0.01, 0.49)
            && bounded(self.pickup_range, 0.0, Self::MAX_PICKUP_RANGE)
            && bounded(self.merge_range, 0.0, 8.0)
            && (1_000..=86_400_000).contains(&self.lifetime_ms)
    }

    pub fn to_bytes(self) -> [u8; Self::BYTE_LEN] {
        let mut bytes = [0; Self::BYTE_LEN];
        for (index, value) in [
            self.gravity,
            self.terminal_speed,
            self.radius,
            self.pickup_range,
            self.merge_range,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes[20..].copy_from_slice(&self.lifetime_ms.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: [u8; Self::BYTE_LEN]) -> Option<Self> {
        let read =
            |index: usize| f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        let policy = Self {
            gravity: read(0),
            terminal_speed: read(1),
            radius: read(2),
            pickup_range: read(3),
            merge_range: read(4),
            lifetime_ms: u64::from_le_bytes(bytes[20..].try_into().unwrap()),
        };
        policy.valid().then_some(policy)
    }
}
