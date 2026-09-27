//! A persistent region clock, independent of entities and their activity.
//! Each owner advances its own phase every fifty logical ticks, through restart.
use bloxgloom_host_api::{RegistrationError, system::*};
use std::sync::Arc;
pub const KEY: &str = "fixture:region_clock";
pub fn definition() -> System {
    System {
        key: KEY.into(),
        schema: 1,
        partition: Partition::Chunk,
        max_state_bytes: 8,
        max_jobs_per_tick: 2,
        after: vec![],
        seeds: vec![
            Seed {
                owner: Owner::Chunk([0, 0, 0]),
                data: 0u64.to_le_bytes().to_vec(),
            },
            Seed {
                owner: Owner::Chunk([1, 0, 0]),
                data: 0u64.to_le_bytes().to_vec(),
            },
        ],
        behavior: Arc::new(Clock),
    }
}
struct Clock;
impl Behavior for Clock {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() != 8 {
            return Err(RegistrationError(
                "region clock requires eight bytes".into(),
            ));
        }
        Ok(())
    }
    fn plan(&self, c: &Context<'_>) -> Result<Plan, RegistrationError> {
        self.validate(c.data)?;
        let current = u64::from_le_bytes(c.data.try_into().unwrap());
        Ok(Plan {
            data: current.wrapping_add(1).to_le_bytes().to_vec(),
            next_tick: c
                .tick
                .checked_add(50)
                .ok_or_else(|| RegistrationError("clock exhausted".into()))?,
        })
    }
}
