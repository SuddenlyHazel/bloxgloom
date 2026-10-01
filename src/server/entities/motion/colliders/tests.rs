use super::*;
use crate::server::entities::{EntityPatch, EntityPayload, EntitySpawn};
use bloxgloom_host_api::{RegistrationError, gameplay::EntityState, motion};
struct Bytes;
impl EntityState for Bytes {
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn public(&self, _: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(vec![])
    }
}
fn declaration() -> MovingEntity {
    MovingEntity {
        key: "demo:projectile".into(),
        schema_version: 1,
        schema_fingerprint: 1,
        max_state_bytes: 1,
        max_public_bytes: 1,
        body: motion::Body {
            half_extents: [0.1; 3],
            collisions: motion::CollisionMask {
                terrain: false,
                players: false,
                creatures: true,
            },
            response: motion::Response::Stop,
            restitution: 0.0,
            gravity_scale: 0.0,
            max_speed: 64.0,
            max_acceleration: 128.0,
        },
        lifetime_ticks: 100,
        interval: 10,
        source_exclusion_ticks: 0,
        handles_impact: true,
        handles_expiry: false,
        model: vec![],
        state: std::sync::Arc::new(Bytes),
    }
}
fn record() -> Record {
    Record {
        simulation_tick: 0,
        motion: motion::Motion {
            position: [4.0, 80.2, 0.0],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            orientation: [0.0, 0.0, 0.0, 1.0],
            revision: 0,
            grounded: false,
        },
        remaining_ticks: 100,
        next_behavior_tick: None,
        source: None,
        source_ticks: 0,
        contact: None,
        contact_normal: None,
        pending: None,
        state: vec![],
    }
}

#[test]
fn moving_capture_uses_committed_creature_crossing_and_fences_target_motion() {
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-relative-capture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut state = crate::server::server_state(19, path.clone()).unwrap();
    let spawn = state
        .entities
        .prepare_spawn(EntitySpawn::Mobile {
            entity_type: crate::content::MOSSBUN_ENTITY_TYPE,
            position: [3.0, 80.0, 0.0],
            payload: EntityPayload::new(crate::server::entities::mossbun::Mossbun::default()),
            spawn_tick: 0,
        })
        .unwrap();
    let id = spawn.entity_id();
    state.entities.apply_committed(spawn).unwrap();
    sample(&mut state, 0);
    let before = state.entities.snapshot(id).unwrap();
    let moved = state
        .entities
        .prepare_update(
            id,
            before.revision,
            EntityPatch {
                payload: None,
                position: Some([5.0, 80.0, 0.0]),
                next_tick: None,
            },
        )
        .unwrap();
    state.entities.apply_committed(moved).unwrap();
    sample(&mut state, 2);
    sample(&mut state, 3);
    sample(&mut state, 4);
    let mut reads = TerrainReads::default();
    let body = declaration();
    let moving = record();
    let candidates = capture(
        &state,
        EntityId::new(999).unwrap(),
        4,
        4,
        &body,
        &moving,
        ([3, 80, -1], [4, 80, 0]),
        &mut reads,
    )
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].displacement, [2.0, 0.0, 0.0]);
    let step = solver::integrate_until_contact(
        solver::State {
            position: [4.0, 80.2, 0.0],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
        },
        solver::Body {
            half_extents: [0.1; 3],
            response: solver::Response::Stop,
            restitution: 0.0,
        },
        2.0 * super::super::DT,
        &candidates,
        solver::Limits {
            colliders: 64,
            sweep_cells: 4096,
            contacts: 4,
            world_min: [-1000.0; 3],
            world_max: [1000.0; 3],
        },
    )
    .unwrap();
    assert_eq!(
        step.contacts.len(),
        1,
        "a collider crossing the whole body is visible even though neither endpoint overlaps"
    );
    assert!(reads.entities_current(&state.entities));
    let mut launch = moving.clone();
    launch.source = Some(id.get());
    launch.source_ticks = 2;
    assert_eq!(
        capture(
            &state,
            EntityId::new(999).unwrap(),
            4,
            4,
            &body,
            &launch,
            ([3, 80, -1], [4, 80, 0]),
            &mut TerrainReads::default()
        )
        .unwrap()
        .len(),
        1,
        "source must remain captured when exclusion expires in the second substep"
    );
    launch.source_ticks = 4;
    assert!(
        capture(
            &state,
            EntityId::new(999).unwrap(),
            4,
            4,
            &body,
            &launch,
            ([3, 80, -1], [4, 80, 0]),
            &mut TerrainReads::default()
        )
        .unwrap()
        .is_empty(),
        "source excluded for the whole horizon need not be offered"
    );
    let snapshot = state.entities.snapshot(id).unwrap();
    let moved = state
        .entities
        .prepare_update(
            id,
            snapshot.revision,
            EntityPatch {
                payload: None,
                position: Some([5.1, 80.0, 0.0]),
                next_tick: None,
            },
        )
        .unwrap();
    state.entities.apply_committed(moved).unwrap();
    assert!(
        !reads.entities_current(&state.entities),
        "stale target pose must force capture retry before admission"
    );
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn moving_history_restart_and_dormancy_do_not_invent_target_motion() {
    let id = EntityId::new(1).unwrap();
    let mut history = History::default();
    history.replace(
        0,
        BTreeMap::from([(
            id,
            Pose {
                position: [1.0, 80.0, 0.0],
            },
        )]),
    );
    assert_eq!(history.start(id, 2, 2, [2.0, 80.0, 0.0]), [1.0, 80.0, 0.0]);
    assert_eq!(history.start(id, 4, 4, [2.0, 80.0, 0.0]), [1.0, 80.0, 0.0]);
    assert_eq!(
        history.start(id, 20, 4, [2.0, 80.0, 0.0]),
        [2.0, 80.0, 0.0],
        "missing active-step history freezes instead of stretching old motion across dormancy"
    );
    for tick in 1..10 {
        history.replace(tick, BTreeMap::new());
    }
    assert_eq!(history.frames.len(), 5);
    assert_eq!(
        History::default().start(id, 4, 4, [2.0, 80.0, 0.0]),
        [2.0, 80.0, 0.0]
    );
}
