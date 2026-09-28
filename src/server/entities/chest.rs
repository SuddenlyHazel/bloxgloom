//! Chest consumes the public storage lifecycle contract, like an extension.
use bloxgloom_host_api::{FootprintCell, StorageBlockEntity};

pub(in crate::server) struct Chest;
impl bloxgloom_host_api::Extension for Chest {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        registrar.storage_block_entity(definition())
    }
}
#[cfg(test)]
pub(in crate::server) type ChestPayload = super::container::ContainerPayload;

pub(in crate::server) fn definition() -> StorageBlockEntity {
    StorageBlockEntity {
        entity: "bloxgloom:chest".into(),
        block: "bloxgloom:chest".into(),
        placement_item: "bloxgloom:chest".into(),
        anchor_state: "bloxgloom:chest".into(),
        slots: 27,
        automation_faces: None,
        footprint: vec![FootprintCell {
            offset: [0; 3],
            state: "bloxgloom:chest".into(),
        }],
    }
}

#[cfg(test)]
pub(in crate::server) fn register(
    builder: &mut super::EntityTypeRegistryBuilder<'_>,
    catalog: &std::sync::Arc<crate::content::Catalog>,
) -> Result<(), super::EntityError> {
    let registry = crate::server::lifecycle::Registry::resolve(catalog, &[definition()])
        .map_err(|_| super::EntityError::InvalidType)?;
    super::container::register(builder, catalog, registry.entries.values().next().unwrap())
}
