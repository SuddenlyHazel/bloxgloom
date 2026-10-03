use super::*;
use bloxgloom_host_api::entity::{self as api, AuthoredModel, VisualState};

fn model() -> bloxgloom_host_api::model::ModelAsset {
    bloxgloom_host_api::model::ModelAsset {
        player: None,
        key: "demo:model".into(),
        glb: include_bytes!("../../../fixtures/authored-model/model.glb").to_vec(),
        controls: Vec::new(),
        scale: 1.0,
    }
}
fn creature() -> MobileEntity {
    let mut creature = crate::content::creatures::mossbun::definition();
    creature.key = "demo:creature".into();
    creature.model.clear();
    creature.authored_model = Some(AuthoredModel {
        key: "demo:model".into(),
        scale: 1.0,
        idle: None,
        walk: None,
        run: None,
    });
    creature
}
#[test]
fn native_authored_bindings_reject_missing_models_and_clips_before_assigning_entity_ids() {
    let mut catalog = Catalog::builtins();
    let creature = creature();
    assert!(
        catalog
            .register_mobile(creature.clone())
            .unwrap_err()
            .0
            .contains("missing authored model")
    );
    assert!(catalog.entity_type_id_by_key(&creature.key).is_none());
    catalog.register_model_asset(&model()).unwrap();
    let mut invalid = creature.clone();
    invalid.authored_model.as_mut().unwrap().walk = Some("missing clip".into());
    assert!(
        catalog
            .register_mobile(invalid)
            .unwrap_err()
            .0
            .contains("unknown model clip")
    );
    assert!(catalog.entity_type_id_by_key(&creature.key).is_none());
    catalog.register_mobile(creature.clone()).unwrap();
    assert!(
        catalog
            .mobile_entity(catalog.entity_type_id_by_key(&creature.key).unwrap())
            .is_some()
    );
}
struct InvalidVisual;
impl api::Behavior for InvalidVisual {
    fn initial(&self) -> api::Payload {
        api::Payload::new(())
    }
    fn encode(&self, _: &api::Payload) -> Result<Vec<u8>, api::Error> {
        Ok(vec![0])
    }
    fn decode(&self, _: &[u8]) -> Result<api::Payload, api::Error> {
        Ok(self.initial())
    }
    fn public(&self, _: &api::Payload) -> Result<Vec<u8>, api::Error> {
        Ok(vec![0])
    }
    fn pose(&self, _: &[u8]) -> Result<api::Pose, api::Error> {
        Ok(api::Pose {
            yaw: 0.0,
            grounded: true,
        })
    }
    fn visual(&self, _: &[u8]) -> Result<Option<VisualState>, api::Error> {
        let mut state = VisualState::default();
        state.layers[0] = 1;
        Ok(Some(state))
    }
    fn tick(&self, _: &api::Context<'_>) -> Result<api::Plan, api::Error> {
        Err(api::Error::InvalidState)
    }
}
#[test]
fn native_initial_visuals_must_match_the_installed_models_control_schema() {
    let mut catalog = Catalog::builtins();
    catalog.register_model_asset(&model()).unwrap();
    let mut creature = creature();
    creature.behavior = Arc::new(InvalidVisual);
    assert!(
        catalog
            .register_mobile(creature.clone())
            .unwrap_err()
            .0
            .contains("model schema")
    );
    assert!(catalog.entity_type_id_by_key(&creature.key).is_none());
    // Binding a preassigned identity is also fail-closed during manifest remap.
    let id = EntityTypeId(catalog.entities.len() as u32);
    catalog
        .register_entity_type(EntityTypeDef {
            id,
            key: creature.key.clone().into(),
            schema_version: creature.schema_version,
            schema_fingerprint: creature.schema_fingerprint,
        })
        .unwrap();
    assert!(catalog.bind_mobile(id, Arc::new(creature)).is_err());
    assert!(catalog.mobile_entity(id).is_none());
}
