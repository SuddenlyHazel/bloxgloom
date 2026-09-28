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
            if definition.read_owner_chunk {
                descriptor = descriptor.read_owner_chunk();
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

struct OwnerChunkView<'a> {
    chunk: &'a crate::world::Chunk,
    catalog: &'a crate::content::Catalog,
}
impl api::WorldRead for OwnerChunkView<'_> {
    fn block(
        &self,
        cell: bloxgloom_host_api::gameplay::Cell,
    ) -> Result<bloxgloom_host_api::gameplay::Block, bloxgloom_host_api::gameplay::Error> {
        let (key, local) = crate::world::world_to_chunk(cell[0], cell[1], cell[2]);
        if key != self.chunk.key {
            return Err(bloxgloom_host_api::gameplay::Error::Unavailable(cell));
        }
        let id = self
            .chunk
            .block(local)
            .ok_or(bloxgloom_host_api::gameplay::Error::Unavailable(cell))?;
        crate::server::gameplay::block(self.catalog, id)
    }
}
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
        let world = job
            .owner_chunk()
            .zip(job.owner_catalog())
            .map(|(chunk, catalog)| OwnerChunkView { chunk, catalog });
        let plan = self
            .0
            .behavior
            .plan(&api::Context {
                owner: public_owner(job.owner()),
                revision: snapshot.revision(),
                tick,
                data,
                world: world.as_ref().map(|world| world as &dyn api::WorldRead),
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
