//! Captured environmental values and their durable override dependencies.
use super::{State, durable::TerrainReads};

#[derive(Clone, Debug)]
pub(super) struct Capture {
    pub value: bloxgloom_host_api::gameplay::Environment,
    clock: super::world_time::ReadStamp,
    weather: super::weather::ReadStamp,
}
impl Capture {
    pub fn new(state: &State) -> Self {
        Self::from_clocks(&state.world_time, &state.weather)
    }
    fn from_clocks(clock: &super::world_time::Clock, weather: &super::weather::Clock) -> Self {
        let clock = clock.capture();
        let weather = weather.capture();
        Self {
            value: bloxgloom_host_api::gameplay::Environment {
                world_time: clock.time,
                weather: weather.weather,
            },
            clock: clock.stamp,
            weather: weather.stamp,
        }
    }
    pub fn clock_capture(&self) -> super::world_time::Capture {
        super::world_time::Capture {
            stamp: self.clock.clone(),
            time: self.value.world_time,
        }
    }
    pub fn weather_capture(&self) -> super::weather::Capture {
        super::weather::Capture {
            stamp: self.weather.clone(),
            weather: self.value.weather,
        }
    }
    pub fn is_current(&self) -> bool {
        self.clock.is_current() && self.weather.is_current()
    }
    pub fn fence(&self, reads: &mut TerrainReads) {
        reads.clock = Some(self.clock.clone());
        reads.weather = Some(self.weather.clone());
    }
}
#[cfg(test)]
mod tests;
