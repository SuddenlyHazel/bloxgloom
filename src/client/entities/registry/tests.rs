use super::*;
use bloxgloom_host_api::entity::{self as api, Behavior};
use std::sync::Arc;

struct NativeProjection;
impl Behavior for NativeProjection {
    fn initial(&self) -> api::Payload {
        api::Payload::new(0u8)
    }
    fn decode(&self, b: &[u8]) -> Result<api::Payload, api::Error> {
        if b.len() == 1 {
            Ok(api::Payload::new(b[0]))
        } else {
            Err(api::Error::InvalidState)
        }
    }
    fn encode(&self, p: &api::Payload) -> Result<Vec<u8>, api::Error> {
        p.downcast_ref::<u8>()
            .map(|v| vec![*v])
            .ok_or(api::Error::InvalidState)
    }
    fn public(&self, p: &api::Payload) -> Result<Vec<u8>, api::Error> {
        self.encode(p)
    }
    fn pose(&self, b: &[u8]) -> Result<api::Pose, api::Error> {
        if b.len() == 1 {
            Ok(api::Pose {
                yaw: 0.,
                grounded: true,
            })
        } else {
            Err(api::Error::InvalidState)
        }
    }
    fn visual(&self, b: &[u8]) -> Result<Option<api::VisualState>, api::Error> {
        let mut v = api::VisualState::default();
        match b {
            [0] => return Ok(None),
            [1] => {
                v.playback = Some(api::ClipPlayback {
                    clip: u16::MAX,
                    speed: 1.,
                    looping: true,
                    crossfade_s: 0.,
                    started_tick: 0,
                    sequence: 0,
                })
            }
            [2] => v.variants[0] = 31,
            [3] => v.layers[31] = 1,
            [4] => {
                v.tints[15] = Some(api::Tint {
                    rgb: [0; 3],
                    mode: api::TintMode::Replace,
                })
            }
            [5] => v.layers[0] = 0,
            _ => return Err(api::Error::InvalidState),
        }
        Ok(Some(v))
    }
    fn tick(&self, _: &api::Context<'_>) -> Result<api::Plan, api::Error> {
        Err(api::Error::InvalidState)
    }
}

#[test]
fn authored_native_updates_are_schema_checked_before_replica_avatar_install() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/glb-creatures/packages/sprout/assets/models");
    let mut catalog = Catalog::builtins();
    catalog
        .register_model_asset(&bloxgloom_host_api::model::ModelAsset {
            player: None,
            key: "demo:model".into(),
            glb: std::fs::read(root.join("sprout.glb")).unwrap(),
            controls: std::fs::read(root.join("looks.json")).unwrap(),
            scale: 1.,
        })
        .unwrap();
    let mut definition = crate::content::creatures::mossbun::definition();
    definition.key = "demo:native".into();
    definition.model.clear();
    definition.max_state_bytes = 1;
    definition.max_public_bytes = 1;
    definition.behavior = Arc::new(NativeProjection);
    definition.authored_model = Some(api::AuthoredModel {
        key: "demo:model".into(),
        scale: 1.,
        idle: Some("idle".into()),
        walk: None,
        run: None,
    });
    catalog.register_mobile(definition).unwrap();
    let kind = catalog.entity_type_id_by_key("demo:native").unwrap();
    let registry = EntityClientRegistry::builtins(&catalog);
    let mut entity = PublicEntity {
        id: 7,
        entity_type: kind,
        revision: 1,
        motion_revision: 1,
        location: crate::protocol::PublicEntityLocation::Mobile {
            position: [0.5, 80., 0.5],
        },
        payload: vec![0],
    };
    let project = |entity: PublicEntity| registry.project(&BTreeMap::from([(entity.id, entity)]));
    assert!(
        project(entity.clone()).unwrap()[0].model_pose.is_none(),
        "native default appearance/locomotion needs no override"
    );
    for bad in 1..=4 {
        entity.payload = vec![bad];
        assert!(
            project(entity.clone()).is_err(),
            "invalid projected control {bad} installed"
        );
    }
    entity.payload = vec![5];
    assert_eq!(project(entity).unwrap()[0].model_pose.unwrap().layers[0], 0);
}
