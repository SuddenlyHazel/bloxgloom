//! Public byte-oriented owner callbacks adapted to the existing worker/WAL path.
use super::*;
use crate::server::parallel::{OwnerJob, OwnerPatch, OwnerSchedule, PatchUsage};
use crate::server::registry::{ResourceId, SystemHandlerError};
use crate::server::runtime::owner_codec::OwnerCodecError;
use crate::server::runtime::owner_effects::OwnerEffectPatch;
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
            if let Some(radius) = definition.read_radius_chunks {
                descriptor = descriptor.read_chunks(radius);
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

struct OwnerWorldView<'a> {
    chunks: &'a [Arc<crate::world::Chunk>],
    catalog: &'a crate::content::Catalog,
}
impl api::WorldRead for OwnerWorldView<'_> {
    fn block(
        &self,
        cell: bloxgloom_host_api::gameplay::Cell,
    ) -> Result<bloxgloom_host_api::gameplay::Block, bloxgloom_host_api::gameplay::Error> {
        let (key, local) = crate::world::world_to_chunk(cell[0], cell[1], cell[2]);
        let index = self
            .chunks
            .binary_search_by_key(&key, |chunk| chunk.key)
            .map_err(|_| bloxgloom_host_api::gameplay::Error::Unavailable(cell))?;
        let chunk = &self.chunks[index];
        let id = chunk
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
        let world = job.owner_catalog().map(|catalog| OwnerWorldView {
            chunks: job.world_chunks(),
            catalog,
        });
        let context = api::Context {
            owner: public_owner(job.owner()),
            revision: snapshot.revision(),
            tick,
            data,
            world: world.as_ref().map(|world| world as &dyn api::WorldRead),
        };
        let plan = self.0.behavior.plan(&context).map_err(|_| reject())?;
        if plan.next_tick <= tick {
            return Err(reject());
        }
        if plan.wakes.len() > 32 {
            return Err(SystemHandlerError::Rejected(
                "public owner system exceeds 32 wakes per job".into(),
            ));
        }
        if plan.edits.len() > 16 {
            return Err(SystemHandlerError::Rejected(
                "public owner system exceeds 16 block edits per job".into(),
            ));
        }
        let mut edited = std::collections::BTreeSet::new();
        for edit in &plan.edits {
            if !matches!(job.owner(), OwnerKey::Chunk(key) if key == crate::world::world_to_chunk(edit.cell[0], edit.cell[1], edit.cell[2]).0)
                || !edited.insert(edit.cell)
                || edit.before == edit.after
                || edit.before.len() > 128
                || edit.after.len() > 128
                || job
                    .owner_catalog()
                    .and_then(|catalog| catalog.state_by_key(&edit.after))
                    .is_none()
                || context
                    .block(edit.cell)
                    .map(|block| block.state != edit.before)
                    .unwrap_or(true)
            {
                return Err(SystemHandlerError::Rejected(
                    "invalid conditional owner block edit".into(),
                ));
            }
        }
        let wakes = plan
            .wakes
            .into_iter()
            .map(|wake| {
                let system = SystemId::new(wake.system).map_err(|_| {
                    SystemHandlerError::Rejected("invalid scheduled system key".into())
                })?;
                Ok((system, internal_owner(wake.owner)))
            })
            .collect::<Result<Vec<_>, SystemHandlerError>>()?;
        let state = self.decode(&plan.data).map_err(|_| reject())?;
        let bytes = plan.data.len()
            + plan
                .edits
                .iter()
                .map(|edit| edit.before.len() + edit.after.len() + 16)
                .sum::<usize>();
        let output = if wakes.is_empty() && plan.edits.is_empty() {
            None
        } else {
            Some(
                OwnerEffectPatch::new(state.clone(), Vec::new())
                    .with_durable_wakes(wakes)
                    .with_world_edits(plan.edits),
            )
        };
        let usage = PatchUsage {
            writes: 1,
            effects: 0,
            estimated_bytes: bytes,
        };
        let patch = if let Some(output) = output {
            OwnerPatch::new(job, output, usage)
        } else {
            OwnerPatch::new(job, state, usage)
        };
        Ok(patch.with_schedule(OwnerSchedule::AtTick(plan.next_tick)))
    }
}
