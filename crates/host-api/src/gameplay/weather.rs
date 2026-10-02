//! Captured server weather; reads are historical inputs, never local forecasts.
use super::{Context, Error};

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
    pub fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Storm => "storm",
            Self::StormMild => "storm_mild",
            Self::StormSevere => "storm_severe",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    /// Target condition, including while transitioning to it.
    pub kind: WeatherKind,
    pub revision: u64,
    pub elapsed_ms: u64,
    pub rain_mm_h: f32,
    pub wind_m_s: f32,
    pub cloud: f32,
    pub transition: f32,
}
impl Weather {
    pub fn valid(self) -> bool {
        self.rain_mm_h.is_finite()
            && (0.0..=54.0).contains(&self.rain_mm_h)
            && self.wind_m_s.is_finite()
            && (0.0..=30.0).contains(&self.wind_m_s)
            && self.cloud.is_finite()
            && (0.0..=1.0).contains(&self.cloud)
            && self.transition.is_finite()
            && (0.0..=1.0).contains(&self.transition)
    }
}
impl Context<'_> {
    /// Repeated reads within this invocation return the same captured input.
    /// Staged admin changes become observable after their durable commit.
    pub fn weather(&mut self) -> Result<Weather, Error> {
        self.charge()?;
        match self.snapshot.weather() {
            Ok(weather) => Ok(weather),
            Err(error) => self.fail(error),
        }
    }
}
