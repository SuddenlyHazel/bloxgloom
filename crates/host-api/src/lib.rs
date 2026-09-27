//! Host contracts for registered storage, inventory screens, and mobile entities.
//! This is not a mod loader or a stable native ABI.
use std::fmt;

pub mod entity;
pub mod inventory;
pub mod lifecycle;
pub use inventory::{InventoryScreen, SlotGroup, StatusField, StatusFormat};
pub use lifecycle::{FootprintCell, StorageBlockEntity};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrationError(pub String);
impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RegistrationError {}

/// Simple opaque cube content. References are canonical namespaced keys, never
/// save/wire integers. Richer state/material definitions are a later capability.
#[derive(Clone, Debug)]
pub struct CubeBlock {
    pub key: String,
    pub name: String,
    pub texture: String,
}

/// Implemented by the host's startup collector. Calls declare content; they do
/// not mutate a running world. Resolution/validation happens before storage opens.
pub trait Registrar {
    fn mobile_entity(&mut self, _entity: entity::MobileEntity) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "mobile entities unsupported by this registrar".into(),
        ))
    }
    fn inventory_screen(&mut self, screen: InventoryScreen) -> Result<(), RegistrationError>;
    fn cube_block(&mut self, block: CubeBlock) -> Result<(), RegistrationError>;
    fn storage_block_entity(&mut self, entity: StorageBlockEntity)
    -> Result<(), RegistrationError>;
}

pub trait Extension {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError>;
}
