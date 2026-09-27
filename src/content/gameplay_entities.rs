use super::{Catalog, EntityTypeDef, EntityTypeId};
use bloxgloom_host_api::{RegistrationError, gameplay::EntityDefinition};
use std::sync::Arc;
impl Catalog {
    pub(crate) fn register_gameplay_entity(
        &mut self,
        definition: EntityDefinition,
    ) -> Result<(), RegistrationError> {
        definition.validate()?;
        if self.gameplay_entities.len() >= 256
            || self.entity_type_id_by_key(&definition.key).is_some()
        {
            return Err(RegistrationError(format!(
                "{}: duplicate or over-limit gameplay entity",
                definition.key
            )));
        }
        let id = EntityTypeId(self.entities.len() as u32);
        self.register_entity_type(EntityTypeDef {
            id,
            key: definition.key.clone().into(),
            schema_version: definition.schema_version,
            schema_fingerprint: definition.schema_fingerprint,
        })
        .map_err(|e| {
            RegistrationError(format!(
                "{}: invalid gameplay entity: {e:?}",
                definition.key
            ))
        })?;
        self.gameplay_entities
            .insert(definition.key.clone(), Arc::new(definition));
        Ok(())
    }
    pub(crate) fn gameplay_entity(&self, key: &str) -> Option<&Arc<EntityDefinition>> {
        self.gameplay_entities.get(key)
    }
    pub(crate) fn gameplay_entities(&self) -> impl Iterator<Item = &Arc<EntityDefinition>> {
        self.gameplay_entities.values()
    }
}
