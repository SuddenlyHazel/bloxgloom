//! Native proposals must obey the same closed launch and reaction lifecycle.
use super::*;
use crate::server::entities::{EntityPayload, EntitySpawn as StoredSpawn, EntityStore};
use bloxgloom_host_api::{
    RegistrationError,
    gameplay::{
        Context, EntityState, Error, Event, EventKind, Handler, HandlerRegistration, MovingSpawn,
    },
    motion::{Body, CollisionMask, Impact, MovingEntity, Pending, Response, Target},
};
use std::sync::Arc;

struct PermissiveState;
impl EntityState for PermissiveState {
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn public(&self, _: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(vec![])
    }
}

struct ImpactOnly;
impl Handler for ImpactOnly {
    fn handle(&self, _: &mut Context<'_>, _: &Event) -> Result<(), Error> {
        Ok(())
    }
}

fn fixture() -> (Arc<Catalog>, EntityStore, MovingSpawn) {
    let mut catalog = Catalog::builtins();
    catalog
        .register_moving(MovingEntity {
            key: "native:bolt".into(),
            schema_version: 1,
            schema_fingerprint: 17,
            max_state_bytes: 1024,
            max_public_bytes: 0,
            body: Body {
                half_extents: [0.1; 3],
                collisions: CollisionMask {
                    terrain: true,
                    players: false,
                    creatures: false,
                },
                response: Response::Bounce,
                restitution: 0.7,
                gravity_scale: 0.0,
                max_speed: 16.0,
                max_acceleration: 32.0,
            },
            physics: None,
            lifetime_ticks: 100,
            interval: 10,
            source_exclusion_ticks: 4,
            handles_impact: true,
            handles_expiry: false,
            model: vec![],
            state: Arc::new(PermissiveState),
        })
        .unwrap();
    catalog
        .register_gameplay_handler(HandlerRegistration {
            key: "native:impact".into(),
            version: 1,
            event: EventKind::MovingImpact,
            target: Some("native:bolt".into()),
            handler: Arc::new(ImpactOnly),
        })
        .unwrap();
    let catalog = Arc::new(catalog);
    let startup = crate::server::startup::ServerStartup::new(catalog.clone());
    let store = EntityStore::new(startup.entity_types_for(catalog.clone()).unwrap());
    let spawn = MovingSpawn {
        key: "native:bolt".into(),
        position: [0.5, 300.5, 0.5],
        velocity: [1.0, 0.0, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        angular_velocity: [0.0; 3],
        state: vec![42],
        source: None,
    };
    (catalog, store, spawn)
}

#[test]
fn moving_native_generic_spawn_cannot_inject_a_valid_host_envelope() {
    let (catalog, store, spawn) = fixture();
    let bytes = services::spawn_record(&catalog, "native", &spawn, 7)
        .unwrap()
        .encode()
        .unwrap();
    // This is a realistic native bypass: both the permissive authored validator
    // and the durable moving codec accept the envelope without a typed launch.
    entities::validate_state(&catalog, &spawn.key, "native", &bytes).unwrap();
    catalog
        .gameplay_entity(&spawn.key)
        .unwrap()
        .state
        .validate(&bytes)
        .unwrap();
    let mut plan = Plan::default();
    plan.entity_spawns.push(EntitySpawn {
        key: spawn.key.clone(),
        position: spawn.position,
        state: bytes,
    });
    plan.moving_spawns.push(spawn.clone());
    let error = prepare(&catalog, &store, &mut plan, 7, None).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("typed launch"));
    assert_eq!(
        plan.moving_spawns.len(),
        1,
        "reject before consuming legitimate launches"
    );

    plan.entity_spawns.clear();
    prepare(&catalog, &store, &mut plan, 7, None).unwrap();
    assert_eq!(plan.entity_spawns.len(), 1);
    let launched = Record::decode(&plan.entity_spawns[0].state).unwrap();
    assert_eq!(launched.state, spawn.state);
    assert_eq!(launched.simulation_tick, 7);
    assert_eq!(launched.remaining_ticks, 100);
}

#[test]
fn moving_native_impact_only_reaction_resumes_physics_without_a_tick_callback() {
    let (catalog, mut store, spawn) = fixture();
    assert!(
        catalog
            .gameplay_handler(EventKind::MovingTick, &spawn.key)
            .is_none()
    );
    let mut record = services::spawn_record(&catalog, "native", &spawn, 7).unwrap();
    let impact = Impact {
        entity: 1,
        motion_revision: record.motion.revision,
        tick: 7,
        position: [0.4, 300.5, 0.5],
        normal: [1.0, 0.0, 0.0],
        incoming_velocity: [-1.0, 0.0, 0.0],
        target: Target::Terrain {
            cell: [-1, 300, 0],
            state: "bloxgloom:stone".into(),
        },
        blocked: false,
    };
    record.pending = Some(Pending::Impact(impact.clone()));
    let allocation = store
        .prepare_spawn(StoredSpawn::Mobile {
            entity_type: catalog.entity_type_id_by_key(&spawn.key).unwrap(),
            position: spawn.position,
            payload: EntityPayload::new(record.encode().unwrap()),
            spawn_tick: 7,
        })
        .unwrap();
    let id = allocation.entity_id();
    assert_eq!(id.get(), impact.entity);
    store.apply_committed(allocation).unwrap();

    let mut plan = Plan::default();
    prepare(
        &catalog,
        &store,
        &mut plan,
        8,
        Some(&Event::MovingImpact { impact }),
    )
    .unwrap();
    let EntityChange::Update { state, .. } = &plan.entity_changes[&id.get()] else {
        panic!("reaction must update its record")
    };
    let resumed = Record::decode(state).unwrap();
    assert!(resumed.pending.is_none());
    assert_eq!(
        resumed.next_behavior_tick, None,
        "an impact-only object must never attempt a missing MovingTick handler"
    );
    assert_eq!(
        resumed.remaining_ticks, record.remaining_ticks,
        "callback consumption does not age active-step lifetime"
    );
    assert_eq!(resumed.motion.velocity, record.motion.velocity);
    assert_eq!(
        plan.entity_schedules[&id.get()],
        Some(10),
        "native integration remains scheduled at the fixed cadence"
    );
    assert!(
        Record::decode(
            store
                .snapshot(id)
                .unwrap()
                .private_payload
                .downcast_ref::<Vec<u8>>()
                .unwrap()
        )
        .unwrap()
        .pending
        .is_some(),
        "planning alone cannot consume a durable impact"
    );
}
