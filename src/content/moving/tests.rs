use super::*;
use bloxgloom_host_api::{
    entity::Cuboid,
    motion::{Body, CollisionMask, Impact, Motion, Pending, Response, Target},
};
struct Bytes;
impl EntityState for Bytes {
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn public(&self, _: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(vec![])
    }
}
#[test]
fn maximum_authored_state_with_long_pending_contact_fits_durable_envelope() {
    let mut catalog = Catalog::builtins();
    catalog
        .register_moving(declaration("demo:projectile"))
        .unwrap();
    let target = Target::Terrain {
        cell: [0, 79, 0],
        state: format!("demo:{}", "a".repeat(123)),
    };
    let motion = Motion {
        position: [0.5, 80.1, 0.5],
        velocity: [0.0; 3],
        acceleration: [0.0; 3],
        orientation: [0.0, 0.0, 0.0, 1.0],
        angular_velocity: [0.0; 3],
        revision: 7,
        grounded: true,
    };
    let record = Record {
        motion,
        simulation_tick: 100,
        remaining_ticks: 1000,
        next_behavior_tick: Some(101),
        source: Some(2),
        source_ticks: 20,
        contact: Some(target.clone()),
        contact_normal: Some([0.0, 1.0, 0.0]),
        pending: Some(Pending::Impact(Impact {
            entity: 1,
            motion_revision: 7,
            tick: 100,
            position: [0.5, 80.0, 0.5],
            normal: [0.0, 1.0, 0.0],
            incoming_velocity: [0.0, -2.0, 0.0],
            target,
            blocked: false,
        })),
        state: vec![0; 65000],
    };
    let bytes = record.encode().unwrap();
    let definition = catalog.gameplay_entity("demo:projectile").unwrap();
    assert!(bytes.len() <= usize::from(definition.max_state_bytes));
    assert!(bytes.len() <= record.state.len() + 512);
    assert_eq!(Record::decode(&bytes).unwrap(), record);
    definition.state.validate(&bytes).unwrap();
    let projection = Projection::decode(&definition.state.public(&bytes).unwrap()).unwrap();
    assert!(projection.data.is_empty());
    assert!(projection.stopped);
    assert_eq!(projection.motion, motion);
}

fn declaration(key: &str) -> MovingEntity {
    MovingEntity {
        key: key.into(),
        schema_version: 1,
        schema_fingerprint: 1,
        max_state_bytes: 65000,
        max_public_bytes: 0,
        body: Body {
            half_extents: [0.1; 3],
            collisions: CollisionMask {
                terrain: true,
                players: false,
                creatures: false,
            },
            response: Response::Stop,
            restitution: 0.0,
            gravity_scale: 1.0,
            max_speed: 64.0,
            max_acceleration: 128.0,
        },
        physics: None,
        lifetime_ticks: 1000,
        interval: 10,
        source_exclusion_ticks: 20,
        handles_impact: true,
        handles_expiry: true,
        model: Vec::<Cuboid>::new(),
        state: Arc::new(Bytes),
    }
}

#[test]
fn moving_catalog_bindings_follow_saved_identity_remapping() {
    let mut saved = Catalog::builtins();
    saved.register_moving(declaration("demo:first")).unwrap();
    saved.register_moving(declaration("demo:second")).unwrap();
    let manifest = crate::content::ContentManifest::from_catalog(&saved);
    let mut local = Catalog::builtins();
    local.register_moving(declaration("demo:second")).unwrap();
    local.register_moving(declaration("demo:first")).unwrap();
    let resolved = manifest.resolve_catalog(&local).unwrap();
    for key in ["demo:first", "demo:second"] {
        let id = saved.entity_type_id_by_key(key).unwrap();
        assert_eq!(resolved.entity_type_id_by_key(key), Some(id));
        assert_eq!(resolved.moving_entity(id).unwrap().key, key);
    }
    assert_eq!(resolved.fingerprint(), saved.fingerprint());
    assert_eq!(
        crate::content::ContentManifest::from_catalog(&resolved),
        manifest
    );
}
