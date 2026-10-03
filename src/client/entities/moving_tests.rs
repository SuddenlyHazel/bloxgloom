use super::*;
use bloxgloom_host_api::motion;

fn projected(position: [f32; 3], tick: u64, revision: u64, data: &[u8]) -> PublicEntity {
    let projection = motion::Projection {
        motion: motion::Motion {
            position,
            velocity: [1.0, 0.0, 0.0],
            acceleration: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            angular_velocity: [0.0; 3],
            revision,
            grounded: false,
        },
        tick,
        stopped: false,
        data: data.to_vec(),
    };
    PublicEntity {
        id: 99,
        entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
        revision: 1,
        motion_revision: revision,
        location: crate::protocol::PublicEntityLocation::Mobile { position },
        payload: projection.encode().unwrap(),
    }
}

#[test]
fn motion_only_replica_commits_allow_envelope_changes_and_preserve_public_state_revision() {
    let before = projected([0.0, 80.0, 0.0], 10, 1, b"visible");
    let after = projected([0.1, 80.0, 0.0], 12, 2, b"visible");
    assert!(valid_successor(&before, &after, true));
    assert!(
        !valid_successor(&before, &after, false),
        "ordinary opaque creature state keeps its old revision contract"
    );
    assert!(!valid_successor(
        &before,
        &projected([0.1, 80.0, 0.0], 12, 2, b"changed"),
        true
    ));
    assert!(!valid_successor(
        &before,
        &projected([0.1, 80.0, 0.0], 9, 2, b"visible"),
        true
    ));
    assert!(!valid_successor(&after, &before, true));
}

#[test]
fn unchanged_motion_revision_cannot_change_pose_or_other_envelope_fields() {
    let before = projected([0.0, 80.0, 0.0], 10, 1, b"visible");
    let changed = projected([0.1, 80.0, 0.0], 12, 1, b"visible");
    assert!(!valid_successor(&before, &changed, true));
    assert!(valid_successor(&before, &before, true));
}

#[test]
fn control_only_publications_advance_full_motion_without_advancing_wire_position_revision() {
    let before = projected([0.0, 80.0, 0.0], 10, 0, b"visible");
    let mut before = PublicEntity {
        motion_revision: 1,
        ..before
    };
    let mut after = before.clone();
    let mut motion = motion::Projection::decode(&after.payload).unwrap();
    motion.motion.velocity = [0.0, 1.0, 0.0];
    motion.motion.revision = 1;
    motion.tick = 12;
    after.payload = motion.encode().unwrap();
    assert!(valid_successor(&before, &after, true));
    assert!(!valid_successor(&after, &before, true));
    before.motion_revision = 2;
    after.motion_revision = 2;
    assert!(
        valid_successor(&before, &after, true),
        "wire positional revision need not equal full motion revision"
    );
}

struct State;
impl bloxgloom_host_api::gameplay::EntityState for State {
    fn validate(&self, _: &[u8]) -> Result<(), bloxgloom_host_api::RegistrationError> {
        Ok(())
    }
    fn public(&self, data: &[u8]) -> Result<Vec<u8>, bloxgloom_host_api::RegistrationError> {
        Ok(data.to_vec())
    }
}
fn catalog() -> Catalog {
    let mut catalog = Catalog::builtins();
    catalog
        .register_moving(motion::MovingEntity {
            key: "demo:projectile".into(),
            schema_version: 1,
            schema_fingerprint: 1,
            max_state_bytes: 16,
            max_public_bytes: 16,
            body: motion::Body {
                half_extents: [0.1; 3],
                collisions: motion::CollisionMask {
                    terrain: true,
                    players: false,
                    creatures: false,
                },
                response: motion::Response::Stop,
                restitution: 0.0,
                gravity_scale: 1.0,
                max_speed: 16.0,
                max_acceleration: 16.0,
            },
            physics: None,
            lifetime_ticks: 100,
            interval: 1,
            source_exclusion_ticks: 0,
            handles_impact: false,
            handles_expiry: false,
            model: vec![bloxgloom_host_api::entity::Cuboid {
                min: [-0.1; 3],
                max: [0.1; 3],
                color: [1.0; 3],
                motion: bloxgloom_host_api::entity::PartMotion::Body,
            }],
            state: std::sync::Arc::new(State),
        })
        .unwrap();
    catalog
}

#[test]
fn moving_projection_validates_redundant_wire_pose_and_exposes_only_authored_public_bytes() {
    let catalog = catalog();
    let mut entity = projected([0.0, 80.0, 0.0], 10, 1, b"visible");
    entity.entity_type = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    let registry = EntityClientRegistry::builtins(&catalog);
    let visuals = registry
        .project(&BTreeMap::from([(entity.id, entity.clone())]))
        .unwrap();
    assert_eq!(
        visuals[0].model,
        crate::render::AvatarModel::Moving(entity.entity_type)
    );
    assert_eq!(visuals[0].motion.unwrap().tick, 10);
    let mut invalid = entity.clone();
    invalid.payload[0] = 99;
    assert!(
        registry
            .project(&BTreeMap::from([(invalid.id, invalid)]))
            .is_err()
    );
    let mut invalid = entity.clone();
    invalid.location = crate::protocol::PublicEntityLocation::Mobile {
        position: [1.0, 80.0, 0.0],
    };
    assert!(
        registry
            .project(&BTreeMap::from([(invalid.id, invalid)]))
            .is_err()
    );
    let mut replicas = Replicas::default();
    let key = crate::world::world_to_chunk(0, 80, 0).0;
    replicas
        .entities
        .insert(key, BTreeMap::from([(entity.id, entity)]));
    let (window, total) = replicas.presentation_entities("demo", &catalog);
    assert_eq!(total, 1);
    assert_eq!(window[0].public, b"visible");
    replicas.retain(|_| false);
    assert!(
        replicas
            .presentation_entities("demo", &catalog)
            .0
            .is_empty()
    );
}
