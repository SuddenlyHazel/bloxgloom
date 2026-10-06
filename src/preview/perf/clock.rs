//! Optional fixed diagnostic wave time; normal benchmarks retain the live clock.
use std::{error::Error, ffi::OsStr};

pub(super) struct WaterClock(Option<f32>);

impl WaterClock {
    pub fn from_env() -> Result<Self, Box<dyn Error>> {
        let value = std::env::var_os("BLOXGLOOM_PERF_WATER_TIME");
        let clock = Self::parse(value.as_deref())?;
        if let Some(time) = clock.0 {
            eprintln!(
                "diagnostic fixed water clock: {time} seconds (near raster, LOD and transport)"
            );
        }
        Ok(clock)
    }

    fn parse(value: Option<&OsStr>) -> Result<Self, String> {
        match value {
            None => Ok(Self(None)),
            Some(value) => value
                .to_str()
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(|value| Self(Some(value)))
                .ok_or_else(|| {
                    "BLOXGLOOM_PERF_WATER_TIME must be a finite, nonnegative number of seconds"
                        .to_string()
                }),
        }
    }

    pub fn sample(&self, realtime: impl FnOnce() -> f32) -> f32 {
        self.0.unwrap_or_else(realtime)
    }
}

#[cfg(test)]
mod tests;
