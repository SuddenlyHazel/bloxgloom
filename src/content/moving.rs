//! Moving entities share durable byte-state storage while exposing authored state
//! separately from the host-owned motion envelope.
use super::{Catalog, EntityTypeId};
use bloxgloom_host_api::{
    RegistrationError,
    gameplay::{EntityDefinition, EntityState},
    motion::{MovingEntity, Projection, Record},
};
use std::sync::Arc;

struct State(Arc<MovingEntity>);
impl EntityState for State {
    fn validate(&self, bytes: &[u8]) -> Result<(), RegistrationError> {
        let record = Record::decode(bytes)?;
        if record.state.len() > usize::from(self.0.max_state_bytes)
            || record.remaining_ticks > self.0.lifetime_ticks
            || record.source_ticks > self.0.source_exclusion_ticks
        {
            return Err(RegistrationError(
                "moving record exceeds declared bounds".into(),
            ));
        }
        validate_motion(&self.0, &record.motion)?;
        self.0.state.validate(&record.state)?;
        if self.0.state.public(&record.state)?.len() > usize::from(self.0.max_public_bytes) {
            return Err(RegistrationError(
                "moving public state exceeds declared bound".into(),
            ));
        }
        Ok(())
    }
    fn public(&self, bytes: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        self.validate(bytes)?;
        let record = Record::decode(bytes)?;
        Projection {
            motion: record.motion,
            tick: record.simulation_tick,
            stopped: record.pending.is_some() || record.motion.velocity == [0.0; 3],
            data: self.0.state.public(&record.state)?,
        }
        .encode()
    }
}
pub(crate) fn validate_motion(
    declaration: &MovingEntity,
    motion: &bloxgloom_host_api::motion::Motion,
) -> Result<(), RegistrationError> {
    motion.validate()?;
    let length = |v: [f32; 3]| v.into_iter().map(|x| x * x).sum::<f32>().sqrt();
    if length(motion.velocity) > declaration.body.max_speed + 0.001
        || length(motion.acceleration) > declaration.body.max_acceleration + 0.001
    {
        return Err(RegistrationError(
            "motion exceeds declared speed/acceleration".into(),
        ));
    }
    Ok(())
}
impl Catalog {
    pub(crate) fn register_moving(
        &mut self,
        declaration: MovingEntity,
    ) -> Result<(), RegistrationError> {
        declaration.validate()?;
        if self.moving_entities().count() >= 128 {
            return Err(RegistrationError("too many moving declarations".into()));
        }
        let declaration = Arc::new(declaration);
        let max_state_bytes = declaration
            .max_state_bytes
            .checked_add(512)
            .ok_or_else(|| {
                RegistrationError("moving state envelope exceeds storage limit".into())
            })?;
        let definition = EntityDefinition {
            key: declaration.key.clone(),
            schema_version: declaration.schema_version,
            schema_fingerprint: declaration.schema_fingerprint,
            max_state_bytes,
            initial_delay_ticks: Some(2),
            state: Arc::new(State(declaration.clone())),
        };
        self.register_gameplay_entity(definition)?;
        let id = self
            .entity_type_id_by_key(&declaration.key)
            .expect("registered moving identity");
        self.bind_moving(id, declaration)
    }
    pub(crate) fn bind_moving(
        &mut self,
        id: EntityTypeId,
        declaration: Arc<MovingEntity>,
    ) -> Result<(), RegistrationError> {
        declaration.validate()?;
        if self
            .entity_type(id)
            .is_none_or(|entity| entity.key != declaration.key)
            || self.moving_entity(id).is_some()
            || self.moving_entities().count() >= 128
        {
            return Err(RegistrationError("invalid moving binding".into()));
        }
        self.moving_entities
            .resize_with(self.entities.len(), || None);
        self.moving_entities[id.0 as usize] = Some(declaration);
        Ok(())
    }
    pub(crate) fn moving_entity(&self, id: EntityTypeId) -> Option<&Arc<MovingEntity>> {
        self.moving_entities.get(id.0 as usize)?.as_ref()
    }
    pub(crate) fn moving_entities(
        &self,
    ) -> impl Iterator<Item = (EntityTypeId, &Arc<MovingEntity>)> {
        self.moving_entities
            .iter()
            .enumerate()
            .filter_map(|(i, value)| value.as_ref().map(|value| (EntityTypeId(i as u32), value)))
    }
}

#[cfg(test)]
mod tests;
