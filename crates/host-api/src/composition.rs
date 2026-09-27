//! Explicit package contracts, checked before catalog installation or save I/O.
//! This is not discovery/loading. A caller registers its complete package bundle.

pub const CONTENT: &str = "bloxgloom:content/v1";
pub const STORAGE: &str = "bloxgloom:storage/v1";
pub const MACHINES: &str = "bloxgloom:machines/v1";
pub const MOBILE_ENTITIES: &str = "bloxgloom:mobile_entities/v1";
pub const INVENTORY_SCREENS: &str = "bloxgloom:inventory_screens/v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub package: String,
    /// Exact contract version. Upgrades are explicit, not a load-order decision.
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub key: String,
    pub version: u32,
    pub dependencies: Vec<Dependency>,
    pub requires: Vec<String>,
}

/// Submit dependent extensions together so declaration order is not resolution
/// order. The host validates the whole bundle atomically. A later installation
/// may depend on earlier packages, but cannot replace their owners or tags.
pub struct Bundle<'a>(pub &'a [&'a dyn crate::Extension]);
impl crate::Extension for Bundle<'_> {
    fn register(
        &self,
        registrar: &mut dyn crate::Registrar,
    ) -> Result<(), crate::RegistrationError> {
        if self.0.len() > 256 {
            return Err(crate::RegistrationError(
                "too many bundled extensions".into(),
            ));
        }
        for extension in self.0 {
            extension.register(registrar)?;
        }
        Ok(())
    }
}
