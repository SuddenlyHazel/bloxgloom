//! Compatibility helpers for built-in spawn and existing behavior tests. The
//! implementation and production registration use the public mobile contract.
use super::*;
#[cfg(test)]
use crate::content::Catalog;
pub(in crate::server) use crate::content::creatures::mossbun::Mossbun;
use crate::server::voxel_view::VoxelView;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
pub(in crate::server) fn register(
    builder: &mut EntityTypeRegistryBuilder<'_>,
    catalog: &Catalog,
) -> Result<(), EntityError> {
    super::mobile::register(
        builder,
        catalog,
        catalog
            .entity_type_id_by_key("bloxgloom:mossbun")
            .ok_or(EntityError::InvalidType)?,
    )
}
#[cfg(test)]
const BODY: super::locomotion::Body = super::locomotion::Body {
    half_width: 0.36,
    height: 0.94,
    speed: 1.5625,
};
#[cfg(test)]
mod tests;
