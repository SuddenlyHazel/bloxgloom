//! Captured own-package motion services and shared transactional record edits.
use crate::{
    content::Catalog,
    server::{
        durable::TerrainReads,
        entities::{EntityId, EntityStore},
    },
};
use bloxgloom_host_api::{
    gameplay::{Error, MotionCommand, MovingSpawn},
    motion::{Motion, MovingEntity, Record},
};

pub(in crate::server) fn declaration<'a>(
    catalog: &'a Catalog,
    key: &str,
    owner: &str,
) -> Result<&'a MovingEntity, Error> {
    if key.split_once(':').map(|x| x.0) != Some(owner) {
        return Err(Error::Invalid("moving entity ownership denied".into()));
    }
    catalog
        .entity_type_id_by_key(key)
        .and_then(|id| catalog.moving_entity(id))
        .map(|x| x.as_ref())
        .ok_or_else(|| Error::Invalid("entity has no motion declaration".into()))
}
pub(in crate::server) fn record(
    catalog: &Catalog,
    store: &EntityStore,
    raw: u64,
    owner: &str,
) -> Result<Option<(Record, String)>, Error> {
    let id = EntityId::new(raw)
        .ok_or_else(|| Error::Invalid("invalid moving entity identity".into()))?;
    let Some(snapshot) = store.snapshot(id) else {
        return Ok(None);
    };
    let key = catalog
        .entity_type(snapshot.entity_type)
        .ok_or_else(|| Error::Host("unknown moving entity identity".into()))?
        .key
        .to_string();
    declaration(catalog, &key, owner)?;
    let bytes = snapshot
        .private_payload
        .downcast_ref::<Vec<u8>>()
        .ok_or_else(|| Error::Host("invalid moving payload".into()))?;
    Ok(Some((
        Record::decode(bytes).map_err(|e| Error::Host(e.0))?,
        key,
    )))
}
pub(in crate::server) fn read(
    catalog: &Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    id: u64,
    owner: &str,
) -> Result<Option<Motion>, Error> {
    let exact =
        EntityId::new(id).ok_or_else(|| Error::Invalid("invalid moving entity identity".into()))?;
    reads
        .entities(store.capture_entity_dependency(exact))
        .map_err(|_| Error::BudgetExceeded)?;
    Ok(record(catalog, store, id, owner)?.map(|x| x.0.motion))
}
pub(in crate::server) fn read_contact(
    catalog: &Catalog,
    reads: &mut TerrainReads,
    store: &EntityStore,
    id: u64,
    owner: &str,
) -> Result<Option<bloxgloom_host_api::motion::MotionContact>, Error> {
    let exact =
        EntityId::new(id).ok_or_else(|| Error::Invalid("invalid moving entity identity".into()))?;
    reads
        .entities(store.capture_entity_dependency(exact))
        .map_err(|_| Error::BudgetExceeded)?;
    Ok(record(catalog, store, id, owner)?.and_then(|x| x.0.contact_state()))
}
pub(in crate::server) fn spawn_record(
    catalog: &Catalog,
    owner: &str,
    spawn: &MovingSpawn,
    tick: u64,
) -> Result<Record, Error> {
    let d = declaration(catalog, &spawn.key, owner)?;
    d.state
        .validate(&spawn.state)
        .map_err(|e| Error::Invalid(e.0))?;
    if spawn.state.len() > usize::from(d.max_state_bytes)
        || d.state
            .public(&spawn.state)
            .map_err(|e| Error::Invalid(e.0))?
            .len()
            > usize::from(d.max_public_bytes)
    {
        return Err(Error::BudgetExceeded);
    }
    let half = d.capture_half_extents();
    if spawn
        .position
        .iter()
        .enumerate()
        .any(|(axis, x)| x.abs() + half[axis] >= 999_998.0)
        || spawn.position[1] - half[1] <= crate::world::BEDROCK_Y as f32
    {
        return Err(Error::Invalid(
            "moving spawn outside supported world bounds".into(),
        ));
    }
    let motion = Motion {
        position: spawn.position,
        velocity: spawn.velocity,
        acceleration: [0.0; 3],
        orientation: spawn.orientation,
        angular_velocity: spawn.angular_velocity,
        revision: 0,
        grounded: false,
    };
    crate::content::moving::validate_motion(d, &motion).map_err(|e| Error::Invalid(e.0))?;
    let record = Record {
        motion,
        simulation_tick: tick,
        remaining_ticks: d.lifetime_ticks,
        next_behavior_tick: if catalog
            .gameplay_handler(
                bloxgloom_host_api::gameplay::EventKind::MovingTick,
                &spawn.key,
            )
            .is_some()
        {
            Some(tick.saturating_add(u64::from(d.interval)))
        } else {
            None
        },
        source: spawn.source,
        source_ticks: if spawn.source.is_some() {
            d.source_exclusion_ticks
        } else {
            0
        },
        contact: None,
        contact_normal: None,
        pending: None,
        state: spawn.state.clone(),
    };
    record.validate().map_err(|e| Error::Invalid(e.0))?;
    Ok(record)
}
pub(in crate::server) fn apply_command(
    catalog: &Catalog,
    key: &str,
    record: &mut Record,
    command: MotionCommand,
) -> Result<(), Error> {
    if record.motion.revision != command.expected_revision {
        return Err(Error::Deferred("moving entity revision changed".into()));
    }
    let declaration = catalog
        .entity_type_id_by_key(key)
        .and_then(|id| catalog.moving_entity(id))
        .ok_or_else(|| Error::Invalid("missing motion declaration".into()))?;
    if declaration.physics.is_some() && command.change.orientation.is_some() {
        return Err(Error::Invalid(
            "rigid body orientation is integrated; use angular_velocity to steer".into(),
        ));
    }
    if let Some(v) = command.change.velocity {
        record.motion.velocity = v;
    }
    if let Some(v) = command.change.acceleration {
        record.motion.acceleration = v;
    }
    if let Some(v) = command.change.orientation {
        record.motion.orientation = v;
    }
    if let Some(v) = command.change.angular_velocity {
        record.motion.angular_velocity = v;
    }
    let d = catalog
        .entity_type_id_by_key(key)
        .and_then(|id| catalog.moving_entity(id))
        .ok_or_else(|| Error::Invalid("missing motion declaration".into()))?;
    crate::content::moving::validate_motion(d, &record.motion).map_err(|e| Error::Invalid(e.0))?;
    record.motion.revision = record
        .motion
        .revision
        .checked_add(1)
        .ok_or_else(|| Error::Invalid("motion revision exhausted".into()))?;
    Ok(())
}
