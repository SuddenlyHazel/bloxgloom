//! A persistent region clock, independent of entities and their activity.
//! Each owner advances its own phase every fifty logical ticks, through restart.
use bloxgloom_host_api::{RegistrationError, system::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub const KEY: &str = "fixture:region_clock";
pub fn definition() -> System {
    System {
        key: KEY.into(),
        schema: 1,
        partition: Partition::Chunk,
        max_state_bytes: 8,
        max_jobs_per_tick: 2,
        read_owner_chunk: false,
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

/// Independent world-read proof, installed only by tests that exercise the
/// owner-chunk capture (not by the general fixture package).
#[derive(Default)]
pub struct WorldProbe {
    /// Advisory signal for loopback tests, not an authoritative gameplay effect.
    pub observed: Option<Arc<AtomicBool>>,
}
impl bloxgloom_host_api::Extension for WorldProbe {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), RegistrationError> {
        registrar.owner_system(System {
            key: "fixture:world_probe".into(),
            schema: 1,
            partition: Partition::Chunk,
            max_state_bytes: 1,
            max_jobs_per_tick: 1,
            read_owner_chunk: true,
            after: vec![],
            seeds: vec![Seed {
                owner: Owner::Chunk([8, 6, 0]),
                data: vec![1],
            }],
            behavior: Arc::new(Probe(self.observed.clone())),
        })
    }
}
struct Probe(Option<Arc<AtomicBool>>);
impl Behavior for Probe {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data.len() != 1 || data[0] > 1 {
            return Err(RegistrationError(
                "world probe state must be one bit".into(),
            ));
        }
        Ok(())
    }
    fn plan(&self, context: &Context<'_>) -> Result<Plan, RegistrationError> {
        let Owner::Chunk([x, y, z]) = context.owner else {
            return Err(RegistrationError("world probe needs a chunk owner".into()));
        };
        let cell = [x * 16, y * 16, z * 16];
        let block = context
            .block(cell)
            .map_err(|error| RegistrationError(error.to_string()))?;
        if !matches!(
            context.block([cell[0] + 16, cell[1], cell[2]]),
            Err(bloxgloom_host_api::gameplay::Error::Unavailable(_))
        ) {
            return Err(RegistrationError(
                "world read escaped the owner chunk".into(),
            ));
        }
        // Negative/air reads are just as authoritative as solid block reads.
        let value = u8::from(block.state == "bloxgloom:sand");
        if let Some(observed) = &self.0 {
            observed.store(true, Ordering::Release);
        }
        Ok(Plan {
            data: vec![value],
            next_tick: context.tick + 2,
        })
    }
}
