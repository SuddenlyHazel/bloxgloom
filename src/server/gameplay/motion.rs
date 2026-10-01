//! Resolve authored motion commands into the existing entity transaction plan.
use super::*;
use crate::server::entities::motion::{self, services};
use bloxgloom_host_api::gameplay::{EntityChange, EntitySpawn, Plan};
use bloxgloom_host_api::motion::Record;

pub(super) fn prepare(
    catalog: &Catalog,
    store: &super::super::entities::EntityStore,
    plan: &mut Plan,
    tick: u64,
    event: Option<&bloxgloom_host_api::gameplay::Event>,
) -> io::Result<()> {
    if plan.entity_spawns.iter().any(|spawn| {
        catalog
            .entity_type_id_by_key(&spawn.key)
            .is_some_and(|id| catalog.moving_entity(id).is_some())
    }) {
        return Err(error(Error::Invalid(
            "moving entities require the typed launch service".into(),
        )));
    }
    let world_count = if plan.moving_spawns.is_empty() {
        0
    } else {
        store
            .mobile_ids_of_types(
                catalog.moving_entities().map(|(id, _)| id),
                motion::MAX_BODIES,
            )
            .map_err(io::Error::other)?
            .len()
    };
    let mut spawned = std::collections::BTreeMap::new();
    for (offset, spawn) in std::mem::take(&mut plan.moving_spawns)
        .into_iter()
        .enumerate()
    {
        let owner = spawn
            .key
            .split_once(':')
            .ok_or_else(|| error(Error::Invalid("missing moving namespace".into())))?
            .0;
        let record = services::spawn_record(catalog, owner, &spawn, tick).map_err(error)?;
        let chunk = crate::world::world_to_chunk(
            spawn.position[0].floor() as i32,
            spawn.position[1].floor() as i32,
            spawn.position[2].floor() as i32,
        )
        .0;
        let chunk_count = store
            .ids_for_chunk(chunk)
            .into_iter()
            .filter(|id| {
                store
                    .snapshot(*id)
                    .is_some_and(|s| catalog.moving_entity(s.entity_type).is_some())
            })
            .count();
        let pending = spawned.entry(chunk).or_insert(0usize);
        if world_count + offset >= motion::MAX_BODIES
            || chunk_count + *pending >= motion::MAX_CHUNK_BODIES
        {
            return Err(error(Error::BudgetExceeded));
        }
        *pending += 1;
        plan.entity_spawns.push(EntitySpawn {
            key: spawn.key,
            position: spawn.position,
            state: record.encode().map_err(|e| error(Error::Invalid(e.0)))?,
        });
    }
    // Replace only the authored payload portion; the motion envelope remains host-owned.
    let reaction = event.and_then(|event| match event {
        bloxgloom_host_api::gameplay::Event::MovingTick { entity, .. }
        | bloxgloom_host_api::gameplay::Event::MovingExpiry { entity, .. } => Some(*entity),
        bloxgloom_host_api::gameplay::Event::MovingImpact { impact } => Some(impact.entity),
        _ => None,
    });
    if let Some(bloxgloom_host_api::gameplay::Event::MovingExpiry { entity, .. }) = event {
        plan.entity_changes
            .insert(*entity, EntityChange::Remove { id: *entity });
    }
    let ids = plan
        .entity_changes
        .keys()
        .copied()
        .chain(plan.motion_commands.keys().copied())
        .chain(reaction)
        .collect::<std::collections::BTreeSet<_>>();
    for raw in ids {
        let id = super::super::entities::EntityId::new(raw)
            .ok_or_else(|| error(Error::Invalid("invalid motion identity".into())))?;
        let Some(snapshot) = store.snapshot(id) else {
            continue;
        };
        if catalog.moving_entity(snapshot.entity_type).is_none() {
            continue;
        }
        if matches!(
            plan.entity_changes.get(&raw),
            Some(EntityChange::Remove { .. })
        ) {
            plan.motion_commands.remove(&raw);
            continue;
        }
        let key = catalog
            .entity_type(snapshot.entity_type)
            .expect("moving identity")
            .key
            .as_ref();
        let before = snapshot
            .private_payload
            .downcast_ref::<Vec<u8>>()
            .ok_or_else(|| error(Error::Host("invalid moving state".into())))?;
        let mut record = Record::decode(before).map_err(|e| error(Error::Host(e.0)))?;
        if let Some(EntityChange::Update { state, .. }) = plan.entity_changes.get(&raw) {
            record.state = state.clone();
        }
        if reaction == Some(raw) {
            record.pending = None;
        }
        if let Some(command) = plan.motion_commands.remove(&raw) {
            services::apply_command(catalog, key, &mut record, command).map_err(error)?;
        }
        if reaction == Some(raw) {
            record.pending = None;
            record.next_behavior_tick = if catalog
                .gameplay_handler(bloxgloom_host_api::gameplay::EventKind::MovingTick, key)
                .is_some()
            {
                Some(
                    tick.saturating_add(u64::from(
                        catalog
                            .moving_entity(snapshot.entity_type)
                            .expect("moving declaration")
                            .interval,
                    )),
                )
            } else {
                None
            };
            record.motion.revision = record
                .motion
                .revision
                .checked_add(1)
                .ok_or_else(|| error(Error::Invalid("motion revision exhausted".into())))?;
        }
        let next = if matches!(event,Some(bloxgloom_host_api::gameplay::Event::MovingTick{entity,..}) if *entity==raw)
        {
            record
                .simulation_tick
                .saturating_add(motion::STEP_TICKS)
                .max(tick.saturating_add(1))
        } else {
            tick.saturating_add(motion::STEP_TICKS)
        };
        plan.entity_schedules.insert(raw, Some(next));
        plan.entity_changes.insert(
            raw,
            EntityChange::Update {
                id: raw,
                state: record.encode().map_err(|e| error(Error::Invalid(e.0)))?,
            },
        );
    }
    Ok(())
}
impl WorldSnapshot<'_> {
    pub(super) fn capture_motion(
        &mut self,
        id: u64,
        owner: &str,
    ) -> Result<Option<bloxgloom_host_api::motion::Motion>, Error> {
        let store = self
            .entities
            .ok_or_else(|| Error::Invalid("motion capture unavailable".into()))?;
        services::read(self.world.catalog(), self.reads, store, id, owner)
    }
    pub(super) fn validate_motion_spawn(
        &mut self,
        owner: &str,
        spawn: &bloxgloom_host_api::gameplay::MovingSpawn,
    ) -> Result<(), Error> {
        services::spawn_record(self.world.catalog(), owner, spawn, self.tick)?;
        // Fence source identity, including absence. An attribution handle must be captured.
        if let Some(raw) = spawn.source {
            let id = super::super::entities::EntityId::new(raw)
                .ok_or_else(|| Error::Invalid("invalid launch source".into()))?;
            let store = self
                .entities
                .ok_or_else(|| Error::Invalid("entity capture unavailable".into()))?;
            if store.snapshot(id).is_none() && !self.players.iter().any(|p| p.entity == raw) {
                return Err(Error::Invalid("launch source is unavailable".into()));
            }
            self.reads
                .entities(store.capture_entity_dependency(id))
                .map_err(|_| Error::BudgetExceeded)?;
        }
        Ok(())
    }
}

/// Validate the complete centered body against the transaction's final terrain.
/// The center-cell generic check alone misses bodies launched through a seam.
pub(super) fn validate_spawn_volume(
    world: &mut World,
    reads: &mut TerrainReads,
    requested: &mut Vec<ChunkKey>,
    catalog: &Catalog,
    final_edits: &[(i32, i32, i32, crate::content::BlockStateId)],
    half_extents: [f32; 3],
    position: [f32; 3],
) -> io::Result<()> {
    let min =
        std::array::from_fn::<_, 3, _>(|axis| (position[axis] - half_extents[axis]).floor() as i32);
    let max = std::array::from_fn::<_, 3, _>(|axis| {
        (position[axis] + half_extents[axis] - 0.00001).floor() as i32
    });
    for x in min[0]..=max[0] {
        for y in min[1]..=max[1] {
            for z in min[2]..=max[2] {
                let at = [x, y, z];
                let block = if let Some(edit) = final_edits
                    .iter()
                    .find(|edit| [edit.0, edit.1, edit.2] == at)
                {
                    edit.3
                } else {
                    let Some(block) = reads.read(world, x, y, z)? else {
                        let key = crate::world::world_to_chunk(x, y, z).0;
                        if !requested.contains(&key) {
                            requested.push(key)
                        }
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "moving spawn volume unavailable",
                        ));
                    };
                    block
                };
                if y <= crate::world::BEDROCK_Y
                    || catalog.block_flags(block) & crate::content::SOLID != 0
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "moving spawn body overlaps terrain",
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
