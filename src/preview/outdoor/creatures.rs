//! Explicit preview-only GLB content, registered through the normal mod catalog.
//! This never changes builtin identities, world schema, or gameplay content.
use super::*;
use bloxgloom_host_api::entity::{AuthoredModel, Tint, TintMode, VisualState};

pub fn install(catalog: &mut crate::content::Catalog) -> Result<(), Box<dyn Error>> {
    catalog.register_model_asset(&bloxgloom_host_api::model::ModelAsset {
        key: "preview:outdoor-creature-model".into(),
        glb: include_bytes!("../../../fixtures/authored-model/model.glb").to_vec(),
        controls: include_bytes!("../../../fixtures/authored-model/controls.json").to_vec(),
        scale: 1.0,
        player: None,
    })?;
    let mut creature = (**catalog
        .mobile_entity(crate::content::MOSSBUN_ENTITY_TYPE)
        .ok_or("missing preview creature body")?)
    .clone();
    creature.key = "preview:outdoor-creature".into();
    creature.interaction.clear();
    creature.model.clear();
    creature.authored_model = Some(AuthoredModel {
        key: "preview:outdoor-creature-model".into(),
        scale: 0.8,
        idle: Some("idle".into()),
        walk: Some("bounce".into()),
        run: None,
    });
    catalog.register_mobile(creature)?;
    Ok(())
}

pub(super) fn append(
    actors: &mut Vec<render::VisualAvatar>,
    chunks: &HashMap<ChunkKey, Arc<world::Chunk>>,
) {
    let Some(kind) = crate::content::catalog().entity_type_id_by_key("preview:outdoor-creature")
    else {
        return;
    };
    let template = actors[0];
    for (index, (position, color)) in [
        ([-8.8, 33.0, -8.7], [220, 145, 80]),
        ([-6.6, 33.0, -9.0], [100, 180, 215]),
        ([2.8, 33.0, -1.8], [200, 120, 190]),
        ([18.5, 33.0, -11.0], [140, 205, 120]),
    ]
    .into_iter()
    .enumerate()
    {
        let position = Vec3::from_array(position);
        let p = (position + Vec3::Y * 0.5).floor().as_ivec3();
        let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
        let light = LightField::build_with_bounce(key, chunks, SEED, false).face(local, 1, 0);
        let mut visual = VisualState {
            transition_s: 0.0,
            ..VisualState::default()
        };
        visual.tints[0] = Some(Tint {
            rgb: color,
            mode: TintMode::Replace,
        });
        println!(
            "outdoor GLB actor {} at {:?}: registered type={} sky={} glow={}",
            10_000 + index as u64,
            position,
            kind.0,
            light.sky,
            light.glow
        );
        actors.push(render::VisualAvatar {
            id: 10_000 + index as u64,
            model: render::AvatarModel::Registered(kind),
            model_pose: Some(visual),
            position,
            light_levels: [light.sky, light.glow, 0, 0],
            glow_color: light.glow_color,
            glow_direction: light.glow_direction,
            pose: [0.0; 4],
            character_recipe: None,
            ..template
        });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_uses_registered_glb_and_generic_contact_body() {
        let mut catalog = crate::content::Catalog::builtins();
        super::install(&mut catalog).unwrap();
        let id = catalog
            .entity_type_id_by_key("preview:outdoor-creature")
            .unwrap();
        let entity = catalog.mobile_entity(id).unwrap();
        assert!(entity.model.is_empty());
        assert!(entity.authored_model.is_some());
        assert!(entity.body.half_width > 0.0);
        assert!(
            catalog
                .model_by_key("preview:outdoor-creature-model")
                .is_some()
        );
    }
}
