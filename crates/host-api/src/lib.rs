//! Host contracts for content, composition, storage, inventories and mobile entities.
//! This is not a mod loader or a stable native ABI.
use std::fmt;

pub mod anchored;
pub mod composition;
pub mod content;
pub mod actions;
pub mod entity;
pub mod inventory;
pub mod lifecycle;
pub mod machine;
pub mod system;
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

/// Convenience shorthand for an opaque cube, one state, and a placeable item.
#[derive(Clone, Debug)]
pub struct CubeBlock {
    pub key: String,
    pub name: String,
    pub texture: String,
}

/// Implemented by the host's startup collector. Calls declare content; they do
/// not mutate a running world. Resolution/validation happens before storage opens.
pub trait Registrar {
    fn owner_system(&mut self, _system: system::System) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "owner systems unsupported by this registrar".into(),
        ))
    }
    fn anchored_block_entity(
        &mut self,
        _entity: anchored::AnchoredBlockEntity,
    ) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "anchored behaviors unsupported by this registrar".into(),
        ))
    }
    fn package(&mut self, _definition: composition::Package) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "package contracts unsupported by this registrar".into(),
        ))
    }
    fn texture(&mut self, _definition: content::Texture) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "textures unsupported by this registrar".into(),
        ))
    }
    fn block(&mut self, _definition: content::Block) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "block definitions unsupported by this registrar".into(),
        ))
    }
    fn item(&mut self, _definition: content::Item) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "item definitions unsupported by this registrar".into(),
        ))
    }
    fn tag(&mut self, _definition: content::Tag) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "tags unsupported by this registrar".into(),
        ))
    }
    fn action(&mut self, _action: actions::Action) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "actions unsupported by this registrar".into(),
        ))
    }
    fn machine(&mut self, _machine: machine::Machine) -> Result<(), RegistrationError> {
        Err(RegistrationError(
            "machines unsupported by this registrar".into(),
        ))
    }
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
