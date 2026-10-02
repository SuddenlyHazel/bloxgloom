//! Historical environmental inputs captured before a scheduled worker runs.
use super::{Weather, WorldTime};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub world_time: WorldTime,
    pub weather: Weather,
}
