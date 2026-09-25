//! Builtin player public-view adapter.
//!
//! One `EntityAdapter` among others: it presents `EntityTypeId(2)` mobile
//! views as avatar visuals and handles no aimed-block interactions. Other
//! entity types register their own adapters without changing this file or the
//! shared wire/commit assembler.

use super::registry::{EntityAdapter, EntityVerb};
use crate::content::Catalog;
use crate::content::EntityTypeId;
use crate::protocol::{ClientMessage, PublicEntity, PublicEntityLocation};
use crate::raycast::Hit;
use crate::render::VisualAvatar;

const PLAYER_ENTITY_TYPE: EntityTypeId = EntityTypeId(2);

pub(super) fn player_adapter() -> EntityAdapter {
    EntityAdapter {
        entity_type: PLAYER_ENTITY_TYPE,
        project_avatar,
        hit_test: no_hit,
        interact: no_interact,
    }
}

fn project_avatar(entity: &PublicEntity) -> Result<Option<VisualAvatar>, ()> {
    let PublicEntityLocation::Mobile { position } = &entity.location else {
        return Err(());
    };
    let cosmetics: [u8; 4] = entity.payload.as_slice().try_into().map_err(|_| ())?;
    Ok(Some(VisualAvatar {
        id: entity.id,
        position: glam::Vec3::from_array(*position),
        cosmetics,
        light_levels: [0; 4],
        bounce: [0; 4],
    }))
}

fn no_hit(_: Hit, _: &Catalog) -> bool {
    false
}

fn no_interact(_: Hit, _: u128, _: u8, _: EntityVerb) -> Option<ClientMessage> {
    None
}
