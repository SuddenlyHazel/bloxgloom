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
        read_radius_chunks: None,
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
            wakes: vec![],
            edits: vec![],
            drops: vec![],
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
            read_radius_chunks: Some(0),
            after: vec![],
            seeds: vec![Seed {
                owner: Owner::Chunk([8, 6, 0]),
                data: vec![1],
            }],
            behavior: Arc::new(Probe(self.observed.clone(), false)),
        })
    }
}
/// A neighboring-chunk probe with a distinct declaration/fingerprint. Its
/// initial destination is deliberately outside the player's startup area.
#[derive(Default)]
pub struct NeighborProbe {
    pub observed: Option<Arc<AtomicBool>>,
}
impl bloxgloom_host_api::Extension for NeighborProbe {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), RegistrationError> {
        registrar.owner_system(System {
            key: "fixture:neighbor_probe".into(),
            schema: 1,
            partition: Partition::Chunk,
            max_state_bytes: 1,
            max_jobs_per_tick: 1,
            read_radius_chunks: Some(1),
            after: vec![],
            seeds: vec![Seed {
                owner: Owner::Chunk([8, 6, 0]),
                data: vec![1],
            }],
            behavior: Arc::new(Probe(self.observed.clone(), true)),
        })
    }
}
struct Probe(Option<Arc<AtomicBool>>, bool);
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
        let query = [cell[0] + if self.1 { 16 } else { 0 }, cell[1], cell[2]];
        let block = context
            .block(query)
            .map_err(|error| RegistrationError(error.to_string()))?;
        if self.1 {
            context
                .block([cell[0] - 1, cell[1], cell[2]])
                .map_err(|error| RegistrationError(error.to_string()))?;
        }
        if !matches!(
            context.block([cell[0] + if self.1 { 32 } else { 16 }, cell[1], cell[2]]),
            Err(bloxgloom_host_api::gameplay::Error::Unavailable(_))
        ) {
            return Err(RegistrationError(
                "world read escaped the declared neighborhood".into(),
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
            wakes: vec![],
            edits: vec![],
            drops: vec![],
        })
    }
}

/// One conditional owner-local placement, for atomic owner/world WAL recovery.
pub struct WorldWriter;
impl bloxgloom_host_api::Extension for WorldWriter {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), RegistrationError> {
        registrar.owner_system(System {
            key: "fixture:world_writer".into(),
            schema: 1,
            partition: Partition::Chunk,
            max_state_bytes: 1,
            max_jobs_per_tick: 1,
            read_radius_chunks: Some(1),
            after: vec![],
            seeds: vec![Seed {
                owner: Owner::Chunk([8, 6, 0]),
                data: vec![0],
            }],
            behavior: Arc::new(Writer),
        })
    }
}
struct Writer;
impl Behavior for Writer {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if !matches!(data, [0] | [1]) {
            return Err(RegistrationError(
                "world writer state must be one bit".into(),
            ));
        }
        Ok(())
    }
    fn plan(&self, context: &Context<'_>) -> Result<Plan, RegistrationError> {
        let cell = [128, 96, 0];
        let block = context
            .block(cell)
            .map_err(|error| RegistrationError(error.to_string()))?;
        Ok(Plan {
            data: vec![1],
            next_tick: context.tick + 1_000,
            wakes: vec![],
            edits: if context.data == [0] && block.state == "bloxgloom:air" {
                vec![BlockEdit {
                    cell,
                    before: block.state,
                    after: "bloxgloom:sand".into(),
                }]
            } else {
                vec![]
            },
            drops: vec![],
        })
    }
}

/// Two independent durable owners: the source schedules a dormant destination
/// once, without sharing state or depending on a transient observer callback.
pub struct WakePair;
impl bloxgloom_host_api::Extension for WakePair {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), RegistrationError> {
        registrar.owner_system(pair_definition("fixture:wake_pair", false))
    }
}
pub struct WakeLoop;
impl bloxgloom_host_api::Extension for WakeLoop {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), RegistrationError> {
        registrar.owner_system(pair_definition("fixture:wake_loop", true))
    }
}
fn pair_definition(key: &str, repeating: bool) -> System {
    System {
        key: key.into(),
        schema: 1,
        partition: Partition::Chunk,
        max_state_bytes: 1,
        max_jobs_per_tick: 2,
        read_radius_chunks: None,
        after: vec![],
        seeds: [8, 9]
            .into_iter()
            .map(|x| Seed {
                owner: Owner::Chunk([x, 6, 0]),
                data: vec![0],
            })
            .collect(),
        behavior: Arc::new(Pair {
            key: key.into(),
            repeating,
        }),
    }
}
struct Pair {
    key: String,
    repeating: bool,
}
impl Behavior for Pair {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if !matches!(data, [0] | [1]) {
            return Err(RegistrationError("wake pair requires one state bit".into()));
        }
        Ok(())
    }

    fn plan(&self, context: &Context<'_>) -> Result<Plan, RegistrationError> {
        let Owner::Chunk([x, 6, 0]) = context.owner else {
            return Err(RegistrationError("wake pair owner mismatch".into()));
        };
        let wakes = if self.repeating || x == 8 && context.data == [0] {
            vec![Wake {
                system: self.key.clone(),
                owner: Owner::Chunk([if x == 8 { 9 } else { 8 }, 6, 0]),
            }]
        } else {
            vec![]
        };
        Ok(Plan {
            data: vec![u8::from(x == 8 || context.revision > 0)],
            next_tick: context.tick + 1_000,
            wakes,
            edits: vec![],
            drops: vec![],
        })
    }
}
