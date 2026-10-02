//! Shared deterministic presentation contract for server-owned weather.
pub(crate) mod luau;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WeatherKind {
    Clear = 0,
    Rain = 1,
    Storm = 2,
    StormMild = 3,
    StormSevere = 4,
}
impl WeatherKind {
    pub(crate) fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Clear),
            1 => Some(Self::Rain),
            2 => Some(Self::Storm),
            3 => Some(Self::StormMild),
            4 => Some(Self::StormSevere),
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
            Self::StormMild => WeatherValues {
                rain: 0.7,
                cloud: 1.,
                wind: 10.,
            },
            Self::StormSevere => WeatherValues {
                rain: 1.8,
                cloud: 1.,
                wind: 30.,
            },
            Self::Storm => WeatherValues {
                rain: 1.,
                cloud: 1.,
                wind: 18.,
            },
        }
    }
}
/// Rain is relative to a normal storm (up to 1.8); cloud is normalized.
/// Wind is metres per second.
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
    pub(crate) fn observation(self, elapsed_ms: u64) -> bloxgloom_host_api::gameplay::Weather {
        use bloxgloom_host_api::gameplay::{Weather, WeatherKind as Kind};
        let sample = self.sample_at(elapsed_ms);
        Weather {
            kind: match self.to {
                WeatherKind::Clear => Kind::Clear,
                WeatherKind::Rain => Kind::Rain,
                WeatherKind::Storm => Kind::Storm,
                WeatherKind::StormMild => Kind::StormMild,
                WeatherKind::StormSevere => Kind::StormSevere,
            },
            revision: self.revision,
            elapsed_ms,
            rain_mm_h: sample.rain * 30.0,
            wind_m_s: sample.wind,
            cloud: sample.cloud,
            transition: if self.transition_duration_ms == 0 {
                1.0
            } else {
                (elapsed_ms.saturating_sub(self.transition_start_ms) as f32
                    / self.transition_duration_ms as f32)
                    .clamp(0.0, 1.0)
            },
        }
    }
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
        if t >= 1.0 {
            return to;
        }
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
        if !matches!(
            self.to,
            WeatherKind::Storm | WeatherKind::StormMild | WeatherKind::StormSevere
        ) {
            return None;
        }
        let slot = elapsed_ms / 15_000;
        let hash = mix(self.seed ^ slot);
        let time = (slot * 15_000).saturating_add(2_000 + hash % 8_000);
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
            && (0. ..=1.8).contains(&self.from.rain)
            && self.from.cloud.is_finite()
            && (0. ..=1.).contains(&self.from.cloud)
            && self.from.wind.is_finite()
            && (0. ..=30.).contains(&self.from.wind)
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
    fn extreme_weather_clock_does_not_overflow_lightning_time() {
        let mut s = WeatherSnapshot::initial(4);
        s.elapsed_ms = u64::MAX - 1;
        s.next_change_ms = u64::MAX;
        s.to = WeatherKind::Storm;
        assert!(s.valid());
        let _ = s.lightning_near(s.elapsed_ms, [0.; 3]);
    }
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
