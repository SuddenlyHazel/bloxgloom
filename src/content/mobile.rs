use super::*;
use bloxgloom_host_api::{RegistrationError as ApiError, entity::MobileEntity};
use std::sync::Arc;
#[cfg(test)]
mod tests;
impl Catalog {
    pub(crate) fn register_mobile(&mut self, entity: MobileEntity) -> Result<(), ApiError> {
        entity.validate()?;
        self.validate_mobile_model(&entity)?;
        if self.entity_type_id_by_key(&entity.key).is_some() {
            return Err(ApiError("duplicate entity identity".into()));
        }
        let id = {
            let id = EntityTypeId(self.entities.len() as u32);
            self.register_entity_type(EntityTypeDef {
                id,
                key: entity.key.clone().into(),
                schema_version: entity.schema_version,
                schema_fingerprint: entity.schema_fingerprint,
            })
            .map_err(|e| ApiError(format!("invalid mobile entity: {e:?}")))?;
            id
        };
        self.bind_mobile(id, Arc::new(entity))
    }
    pub(crate) fn bind_mobile(
        &mut self,
        id: EntityTypeId,
        entity: Arc<MobileEntity>,
    ) -> Result<(), ApiError> {
        entity.validate()?;
        self.validate_mobile_model(&entity)?;
        if self.mobile_entities().count() >= 128 {
            return Err(ApiError("too many mobile models".into()));
        }
        if self.entity_type(id).is_none_or(|d| {
            d.key != entity.key
                || d.schema_version != entity.schema_version
                || d.schema_fingerprint != entity.schema_fingerprint
        }) {
            return Err(ApiError("incompatible mobile identity".into()));
        }
        self.mobile_entities
            .resize_with(self.entities.len(), || None);
        let slot = &mut self.mobile_entities[id.0 as usize];
        if slot.is_some() {
            return Err(ApiError("duplicate mobile implementation".into()));
        }
        let action = bloxgloom_host_api::actions::Action {
            key: format!("{}/interact", entity.key),
            version: 1,
            label: "INTERACT".into(),
            target: bloxgloom_host_api::actions::Target::Entity(entity.key.clone()),
            operation: bloxgloom_host_api::actions::Operation::EntityRequest(
                entity.interaction.clone(),
            ),
            panel: None,
            command: None,
        };
        *slot = Some(entity);
        if !matches!(&action.operation, bloxgloom_host_api::actions::Operation::EntityRequest(bytes) if bytes.is_empty())
        {
            self.register_action(action)?;
        }
        Ok(())
    }
    fn validate_mobile_model(&self, entity: &MobileEntity) -> Result<(), ApiError> {
        let Some(authored) = &entity.authored_model else {
            return Ok(());
        };
        let asset = self.model_by_key(&authored.key).ok_or_else(|| {
            ApiError(format!(
                "{}: missing authored model {}",
                entity.key, authored.key
            ))
        })?;
        let schema = asset.visual_schema();
        if [&authored.idle, &authored.walk, &authored.run]
            .into_iter()
            .flatten()
            .any(|clip| !schema.clips.contains(clip))
        {
            return Err(ApiError(format!(
                "{}: locomotion references an unknown model clip",
                entity.key
            )));
        }
        let initial = entity.behavior.initial();
        let fail = || {
            ApiError(format!(
                "{}: initial state does not match authored model schema",
                entity.key
            ))
        };
        let private = entity.behavior.encode(&initial).map_err(|_| fail())?;
        let public = entity.behavior.public(&initial).map_err(|_| fail())?;
        let pose = entity.behavior.pose(&public).map_err(|_| fail())?;
        let visual = entity.behavior.visual(&public).map_err(|_| fail())?;
        if private.len() > entity.max_state_bytes
            || public.len() > entity.max_public_bytes
            || !pose.yaw.is_finite()
            || visual.as_ref().is_some_and(|state| !schema.accepts(state))
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(crate) fn mobile_entity(&self, id: EntityTypeId) -> Option<&Arc<MobileEntity>> {
        self.mobile_entities.get(id.0 as usize)?.as_ref()
    }
    pub(crate) fn mobile_entities(
        &self,
    ) -> impl Iterator<Item = (EntityTypeId, &Arc<MobileEntity>)> {
        self.mobile_entities
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.as_ref().map(|e| (EntityTypeId(i as u32), e)))
    }
}
