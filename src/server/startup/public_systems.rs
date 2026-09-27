//! Public byte-oriented owner callbacks adapted to the existing worker/WAL path.
use super::*;
use crate::server::parallel::{OwnerJob, OwnerPatch, OwnerSchedule, PatchUsage};
use crate::server::registry::{ResourceId, SystemHandlerError};
use crate::server::runtime::owner_codec::OwnerCodecError;
use crate::server::simulation::Phase;
use bloxgloom_host_api::system as api;

impl ServerStartup {
    pub(super) fn install_public_systems(&mut self) {
        let definitions = self.catalog.owner_systems().cloned().collect::<Vec<_>>();
        for definition in definitions {
            let id = SystemId::new(&definition.key).expect("validated system key");
            if self.systems.iter().any(|(s, _)| s.id() == &id) {
                continue;
            }
            let partition = match definition.partition {
                api::Partition::Chunk => OwnerPartition::Chunk,
                api::Partition::Entity => OwnerPartition::Entity,
                api::Partition::Profile => OwnerPartition::Profile,
            };
            let mut descriptor = SystemDescriptor::new(
                id.clone(),
                Phase::Simulation,
                partition,
                definition.max_jobs_per_tick as usize,
                0,
            )
            .write(ResourceId::new(&definition.key).expect("validated resource key"));
            for after in &definition.after {
                descriptor = descriptor.after(SystemId::new(after).expect("validated dependency"));
            }
            self.systems
                .push((descriptor, Arc::new(Adapter(definition.clone()))));
            self.register_owner_codec(
                id.clone(),
                StartupOwnerCodec {
                    codec: Arc::new(Adapter(definition.clone())),
                    codec_version: 1,
                    max_bytes: definition.max_state_bytes as usize,
                },
            );
            for seed in &definition.seeds {
                self.seed_owner(id.clone(), internal_owner(seed.owner), seed.data.clone());
            }
        }
    }
}

fn internal_owner(owner: api::Owner) -> OwnerKey {
    match owner {
        api::Owner::Chunk([x, y, z]) => OwnerKey::Chunk(crate::world::ChunkKey { x, y, z }),
        api::Owner::Entity(id) => OwnerKey::Entity(id),
        api::Owner::Profile(id) => OwnerKey::Profile(id),
    }
}
fn public_owner(owner: OwnerKey) -> api::Owner {
    match owner {
        OwnerKey::Chunk(c) => api::Owner::Chunk([c.x, c.y, c.z]),
        OwnerKey::Entity(id) => api::Owner::Entity(id),
        OwnerKey::Profile(id) => api::Owner::Profile(id),
    }
}
struct Adapter(Arc<api::System>);
#[cfg(test)]
mod tests;
impl OwnerValueCodec for Adapter {
    fn decode(&self, bytes: &[u8]) -> Result<OwnerData, OwnerCodecError> {
        if bytes.len() > self.0.max_state_bytes as usize {
            return Err(OwnerCodecError::InvalidData);
        }
        self.0
            .behavior
            .validate(bytes)
            .map_err(|_| OwnerCodecError::InvalidData)?;
        Ok(OwnerData::new(bytes.to_vec()))
    }
    fn encode(&self, value: &OwnerData) -> Result<Vec<u8>, OwnerCodecError> {
        let bytes = value.get::<Vec<u8>>().ok_or(OwnerCodecError::InvalidData)?;
        self.decode(bytes)?;
        Ok(bytes.clone())
    }
}
impl SystemHandler for Adapter {
    fn prepare(&self, job: &OwnerJob) -> Result<OwnerPatch, SystemHandlerError> {
        let reject = || SystemHandlerError::Rejected("invalid public owner-system result".into());
        let snapshot = job.snapshot(job.owner()).ok_or_else(reject)?;
        let data = snapshot
            .value::<OwnerData>()
            .and_then(|data| data.get::<Vec<u8>>())
            .ok_or_else(reject)?;
        let tick = job.key().batch.tick().get();
        let plan = self
            .0
            .behavior
            .plan(&api::Context {
                owner: public_owner(job.owner()),
                revision: snapshot.revision(),
                tick,
                data,
            })
            .map_err(|_| reject())?;
        if plan.next_tick <= tick {
            return Err(reject());
        }
        let state = self.decode(&plan.data).map_err(|_| reject())?;
        Ok(OwnerPatch::new(
            job,
            state,
            PatchUsage {
                writes: 1,
                effects: 0,
                estimated_bytes: plan.data.len(),
            },
        )
        .with_schedule(OwnerSchedule::AtTick(plan.next_tick)))
    }
}
