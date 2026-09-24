//! Builtin player public-view adapter. Modded entities can register other
//! visual adapters without changing the shared wire or commit assembler.

use crate::content::EntityTypeId;
use crate::protocol::{PublicEntity, PublicEntityLocation};
use crate::render::VisualAvatar;
use glam::Vec3;
use std::collections::BTreeMap;

const PLAYER_ENTITY_TYPE: EntityTypeId = EntityTypeId(2);

pub(super) fn project(entities: &BTreeMap<u64, PublicEntity>) -> Result<Vec<VisualAvatar>, ()> {
    let mut avatars = Vec::new();
    for entity in entities.values() {
        if entity.entity_type != PLAYER_ENTITY_TYPE {
            continue;
        }
        let PublicEntityLocation::Mobile { position } = &entity.location else {
            return Err(());
        };
        let cosmetics: [u8; 4] = entity.payload.as_slice().try_into().map_err(|_| ())?;
        avatars.push(VisualAvatar {
            id: entity.id,
            position: Vec3::from_array(*position),
            cosmetics,
            light_levels: [0; 4],
            bounce: [0; 4],
        });
    }
    Ok(avatars)
}
