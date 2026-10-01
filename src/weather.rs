//! Shared deterministic presentation contract for server-owned weather.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WeatherKind {
    Clear = 0,
    Rain = 1,
    Storm = 2,
}
impl WeatherKind {
    pub(crate) fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Clear),
            1 => Some(Self::Rain),
            2 => Some(Self::Storm),
            _ => None,
        }
    }
    pub(crate) fn values(self) -> WeatherValues {
        match self {
            Self::Clear => WeatherValues {
                rain: 0.,
                cloud: 0.,
                wind: 2.,
            },
            Self::Rain => WeatherValues {
                rain: 0.6,
                cloud: 0.75,
                wind: 6.,
            },
            Self::Storm => WeatherValues {
                rain: 1.,
                cloud: 1.,
                wind: 18.,
            },
        }
    }
}
/// Rain and cloud are normalized; wind is metres per second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherValues {
    pub rain: f32,
    pub cloud: f32,
    pub wind: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherSnapshot {
    pub elapsed_ms: u64,
    pub seed: u64,
    pub from: WeatherValues,
    pub to: WeatherKind,
    pub transition_start_ms: u64,
    pub transition_duration_ms: u32,
    pub next_change_ms: u64,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lightning {
    pub id: u64,
    pub elapsed_ms: u64,
    pub position: [f32; 3],
}
impl WeatherSnapshot {
    pub(crate) fn initial(seed: u64) -> Self {
        Self {
            elapsed_ms: 0,
            seed,
            from: WeatherKind::Clear.values(),
            to: WeatherKind::Clear,
            transition_start_ms: 0,
            transition_duration_ms: 0,
            next_change_ms: 180_000,
            revision: 0,
        }
    }
    pub(crate) fn sample_at(self, elapsed_ms: u64) -> WeatherValues {
        let t = if self.transition_duration_ms == 0 {
            1.
        } else {
            (elapsed_ms.saturating_sub(self.transition_start_ms) as f32
                / self.transition_duration_ms as f32)
                .clamp(0., 1.)
        };
        let t = t * t * (3. - 2. * t);
        let to = self.to.values();
        WeatherValues {
            rain: self.from.rain + (to.rain - self.from.rain) * t,
            cloud: self.from.cloud + (to.cloud - self.from.cloud) * t,
            wind: self.from.wind + (to.wind - self.from.wind) * t,
        }
    }
    #[cfg(test)]
    pub(crate) fn lightning_at(self, elapsed_ms: u64) -> Option<Lightning> {
        self.lightning_near(elapsed_ms, [0.; 3])
    }
    /// The seed and clock define one world-space strike per 512 m region.
    /// Players in the same region agree; no strike is attached to the camera.
    pub(crate) fn lightning_near(self, elapsed_ms: u64, listener: [f32; 3]) -> Option<Lightning> {
        if listener
            .iter()
            .any(|v| !v.is_finite() || v.abs() >= 1_000_000.)
        {
            return None;
        }
        if self.to != WeatherKind::Storm {
            return None;
        }
        let slot = elapsed_ms / 15_000;
        let hash = mix(self.seed ^ slot);
        let time = slot * 15_000 + 2_000 + hash % 8_000;
        if elapsed_ms < time
            || time
                < self
                    .transition_start_ms
                    .saturating_add(u64::from(self.transition_duration_ms))
        {
            return None;
        }
        let region_x = (listener[0] / 512.).floor() as i32;
        let region_z = (listener[2] / 512.).floor() as i32;
        let region_hash = mix(hash ^ (region_x as u32 as u64) ^ ((region_z as u32 as u64) << 32));
        Some(Lightning {
            id: slot + 1,
            elapsed_ms: time,
            position: [
                region_x as f32 * 512. + (region_hash % 512) as f32,
                120.,
                region_z as f32 * 512. + ((region_hash >> 32) % 512) as f32,
            ],
        })
    }
    pub(crate) fn valid(self) -> bool {
        self.from.rain.is_finite()
            && (0. ..=1.).contains(&self.from.rain)
            && self.from.cloud.is_finite()
            && (0. ..=1.).contains(&self.from.cloud)
            && self.from.wind.is_finite()
            && (0. ..=18.).contains(&self.from.wind)
            && self.transition_duration_ms <= 60_000
            && self.transition_start_ms <= self.elapsed_ms
            && self.next_change_ms > self.elapsed_ms
            && self.next_change_ms.saturating_sub(self.elapsed_ms) <= 360_000
    }
}
pub(crate) fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regional_strikes_are_stable_world_positions_near_far_and_negative_players() {
        let mut s = WeatherSnapshot::initial(4);
        s.to = WeatherKind::Storm;
        let a = s.lightning_near(29_000, [-510., 40., -10.]).unwrap();
        let b = s.lightning_near(29_000, [-1., 60., -1.]).unwrap();
        assert_eq!(a, b);
        assert!((-512. ..0.).contains(&a.position[0]));
        assert!((-512. ..0.).contains(&a.position[2]));
        let far = s.lightning_near(29_000, [100_010., 80., 200_010.]).unwrap();
        assert_eq!(a.id, far.id);
        assert_eq!(a.elapsed_ms, far.elapsed_ms);
        assert!((far.position[0] - 100_010.).abs() <= 512.);
        assert!((far.position[2] - 200_010.).abs() <= 512.);
        assert!(s.lightning_near(29_000, [f32::NAN, 0., 0.]).is_none());
    }
    #[test]
    fn transitions_are_continuous_and_lightning_is_shared() {
        let mut s = WeatherSnapshot::initial(4);
        s.to = WeatherKind::Storm;
        s.transition_duration_ms = 10_000;
        assert_eq!(s.sample_at(0).rain, 0.);
        assert_eq!(s.sample_at(5_000).rain, 0.5);
        assert_eq!(s.sample_at(10_000).rain, 1.);
        assert!(s.lightning_at(5_000).is_none());
        assert_eq!(s.lightning_at(29_000), s.lightning_at(29_000));
    }
}
pub(crate) mod codec;
