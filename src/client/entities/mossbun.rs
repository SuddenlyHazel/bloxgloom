use super::registry::EntityAdapter;
use crate::content::EntityTypeId;
use crate::protocol::{PublicEntity, PublicEntityLocation};
use crate::render::{AvatarModel, VisualAvatar};

pub(super) fn adapter(entity_type: EntityTypeId) -> EntityAdapter {
    EntityAdapter {
        entity_type,
        project_avatar: project,
        hit_test: |_, _| false,
        interact: |_, _, _, _| None,
    }
}

fn project(entity: &PublicEntity) -> Result<Option<VisualAvatar>, ()> {
    let PublicEntityLocation::Mobile { position } = entity.location else {
        return Err(());
    };
    let [facing, walking] = entity.payload.as_slice() else {
        return Err(());
    };
    if *facing > 3
        || *walking > 1
        || position
            .iter()
            .any(|v| !v.is_finite() || v.abs() >= 999_999.0)
        || position[1] <= crate::world::BEDROCK_Y as f32
    {
        return Err(());
    }
    Ok(Some(VisualAvatar {
        model: AvatarModel::Mossbun,
        pose: [
            f32::from(*facing) * std::f32::consts::FRAC_PI_2,
            if *walking == 1 {
                ((position[0] + position[2]) * std::f32::consts::TAU).sin()
            } else {
                0.0
            },
        ],
        id: entity.id,
        position: glam::Vec3::from_array(position),
        cosmetics: [0; 4],
        light_levels: [0; 4],
        bounce: [0; 4],
    }))
}
