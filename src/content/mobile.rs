use super::*;
use bloxgloom_host_api::{RegistrationError as ApiError, entity::MobileEntity};
use std::sync::Arc;
impl Catalog {
    pub(crate) fn register_mobile(&mut self, entity: MobileEntity) -> Result<(), ApiError> {
        entity.validate()?;
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
        };
        *slot = Some(entity);
        if !matches!(&action.operation, bloxgloom_host_api::actions::Operation::EntityRequest(bytes) if bytes.is_empty())
        {
            self.register_action(action)?;
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
