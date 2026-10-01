//! Translate owned motion proposals to existing fenced entity participants.
use super::*;
use crate::server::entities::motion::services;
use bloxgloom_host_api::motion::Record;
fn reject(message: impl Into<String>) -> SystemHandlerError {
    SystemHandlerError::Rejected(message.into())
}
pub(super) fn prepare(
    plan: &mut api::Plan,
    context: &api::Context<'_>,
    job: &OwnerJob,
    system: &api::System,
) -> Result<(), SystemHandlerError> {
    if plan.moving_spawns.len() + plan.entity_spawns.len() > 16
        || !plan.moving_spawns.is_empty() && !system.behavior.creates_moving_entities()
        || plan.motion_commands.len() > 16
        || !plan.motion_commands.is_empty() && !system.behavior.mutates_motion()
    {
        return Err(reject("moving owner operation authority/budget exceeded"));
    }
    let namespace = system.key.split_once(':').map_or("", |x| x.0);
    let catalog = job.owner_catalog();
    for spawn in std::mem::take(&mut plan.moving_spawns) {
        if spawn.state.len() > 1024 {
            return Err(reject("owner moving payload exceeds 1024 bytes"));
        }
        let catalog = catalog.ok_or_else(|| reject("moving owner catalog missing"))?;
        let record = services::spawn_record(catalog, namespace, &spawn, context.tick)
            .map_err(|e| reject(e.to_string()))?;
        let body = services::declaration(catalog, &spawn.key, namespace)
            .map_err(|e| reject(e.to_string()))?
            .body;
        // Validate complete body against the captured neighborhood before staging.
        for x in (spawn.position[0] - body.half_extents[0]).floor() as i32
            ..=(spawn.position[0] + body.half_extents[0]).ceil() as i32 - 1
        {
            for y in (spawn.position[1] - body.half_extents[1]).floor() as i32
                ..=(spawn.position[1] + body.half_extents[1]).ceil() as i32 - 1
            {
                for z in (spawn.position[2] - body.half_extents[2]).floor() as i32
                    ..=(spawn.position[2] + body.half_extents[2]).ceil() as i32 - 1
                {
                    context
                        .block([x, y, z])
                        .map_err(|e| reject(e.to_string()))?;
                }
            }
        }
        if let Some(source) = spawn.source
            && context
                .entities()
                .is_none_or(|entities| !entities.iter().any(|entity| entity.id == source))
        {
            return Err(reject("launch source was not captured"));
        }
        plan.entity_spawns.push(api::EntitySpawn {
            key: spawn.key,
            position: spawn.position,
            state: record.encode().map_err(|e| reject(e.0))?,
        });
    }
    let snapshots = job.world_entities().unwrap_or(&[]);
    let mut records = std::collections::BTreeMap::new();
    for change in &mut plan.entity_changes {
        let api::EntityChange::Update { id, state, .. } = change else {
            continue;
        };
        let Some(snapshot) = snapshots.iter().find(|s| s.id.get() == *id) else {
            return Err(reject("owner entity was not captured"));
        };
        if catalog.is_some_and(|catalog| catalog.moving_entity(snapshot.entity_type).is_some()) {
            if state.len() > 1024 {
                return Err(reject("owner moving payload exceeds 1024 bytes"));
            }
            let mut record = Record::decode(
                snapshot
                    .private_payload
                    .downcast_ref::<Vec<u8>>()
                    .ok_or_else(|| reject("invalid moving record"))?,
            )
            .map_err(|e| reject(e.0))?;
            record.state = state.clone();
            records.insert(*id, record);
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for command in std::mem::take(&mut plan.motion_commands) {
        if !seen.insert(command.id) {
            return Err(reject("duplicate owner motion command"));
        }
        let offered = context
            .entities()
            .and_then(|entities| entities.iter().find(|entity| entity.id == command.id))
            .ok_or_else(|| reject("motion target was not captured"))?;
        if plan
            .entity_changes
            .iter()
            .any(|change| matches!(change,api::EntityChange::Remove{id,..} if *id==command.id))
        {
            return Err(reject("motion control conflicts with removal"));
        }
        let snapshot = snapshots
            .iter()
            .find(|s| s.id.get() == command.id)
            .ok_or_else(|| reject("missing motion snapshot"))?;
        let mut record = records.remove(&command.id).map(Ok).unwrap_or_else(|| {
            Record::decode(
                snapshot
                    .private_payload
                    .downcast_ref::<Vec<u8>>()
                    .ok_or_else(|| reject("invalid motion payload"))?,
            )
            .map_err(|e| reject(e.0))
        })?;
        if record.pending.is_some() {
            return Err(reject("motion reaction pending"));
        }
        services::apply_command(
            catalog.ok_or_else(|| reject("missing motion catalog"))?,
            &offered.key,
            &mut record,
            command,
        )
        .map_err(|e| reject(e.to_string()))?;
        records.insert(command.id, record);
        if !plan
            .entity_changes
            .iter()
            .any(|change| change.id() == command.id)
        {
            plan.entity_changes.push(api::EntityChange::Update {
                id: command.id,
                before_revision: offered.revision,
                state: vec![],
            });
        }
    }
    for change in &mut plan.entity_changes {
        if let api::EntityChange::Update { id, state, .. } = change
            && let Some(record) = records.remove(id)
        {
            *state = record.encode().map_err(|e| reject(e.0))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "motion/tests.rs"]
mod tests;
