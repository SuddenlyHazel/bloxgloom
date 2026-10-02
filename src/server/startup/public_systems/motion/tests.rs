use super::*;
use crate::server::{
    parallel::{BatchId, JobKey, OwnerSnapshot},
    simulation::TickId,
};
use bloxgloom_host_api::{
    RegistrationError,
    gameplay::{EntityState, MovingSpawn},
    motion::{Body, CollisionMask, MovingEntity, Record, Response},
};
struct Bytes;
impl EntityState for Bytes {
    fn validate(&self, data: &[u8]) -> Result<(), RegistrationError> {
        if data == [1] {
            Ok(())
        } else {
            Err(RegistrationError("wrong private state".into()))
        }
    }
    fn public(&self, _: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        Ok(vec![])
    }
}
struct MotionOwner(bool);
impl api::Behavior for MotionOwner {
    fn creates_moving_entities(&self) -> bool {
        self.0
    }
    fn validate(&self, _: &[u8]) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn plan(&self, c: &api::Context<'_>) -> Result<api::Plan, RegistrationError> {
        Ok(api::Plan {
            data: vec![],
            next_tick: c.tick + 1,
            wakes: vec![],
            edits: vec![],
            drops: vec![],
            entity_spawns: vec![],
            entity_changes: vec![],
            moving_spawns: vec![],
            motion_commands: vec![],
        })
    }
}
fn catalog() -> Arc<Catalog> {
    let mut c = Catalog::builtins();
    c.register_moving(MovingEntity {
        key: "fixture:bolt".into(),
        schema_version: 1,
        schema_fingerprint: 9,
        max_state_bytes: 1,
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
            max_speed: 8.0,
            max_acceleration: 8.0,
        },
        lifetime_ticks: 100,
        interval: 3,
        source_exclusion_ticks: 0,
        handles_impact: false,
        handles_expiry: false,
        model: vec![],
        state: Arc::new(Bytes),
    })
    .unwrap();
    Arc::new(c)
}
fn spawn() -> MovingSpawn {
    MovingSpawn {
        key: "fixture:bolt".into(),
        position: [0.5; 3],
        velocity: [1.0, 0.0, 0.0],
        orientation: [0.0, 0.0, 0.0, 1.0],
        state: vec![1],
        source: None,
    }
}
#[test]
fn owner_moving_spawn_wraps_record_and_rejects_missing_authority_or_capture() {
    let catalog = catalog();
    let key = crate::world::ChunkKey { x: 0, y: 0, z: 0 };
    let owner = OwnerKey::Chunk(key);
    let chunks = vec![Arc::new(crate::world::Chunk::from_blocks(
        key,
        1,
        vec![crate::world::AIR; crate::world::CHUNK_SIZE.pow(3)],
    ))];
    let job = OwnerJob::new(
        SystemId::new("fixture:motion").unwrap(),
        JobKey::new(
            BatchId::new(TickId::new(50), Phase::Simulation, 0),
            owner,
            0,
            7,
        ),
        vec![OwnerSnapshot::new(
            owner,
            7,
            Arc::new(OwnerData::new(vec![] as Vec<u8>)),
        )],
    )
    .unwrap()
    .with_world_chunks(chunks.clone(), catalog.clone());
    let view = OwnerWorldView {
        chunks: &chunks,
        entities: None,
        motion: Default::default(),
        contacts: Default::default(),
        catalog: &catalog,
    };
    let context = api::Context {
        environment: None,
        tags: None,
        owner: api::Owner::Chunk([0; 3]),
        revision: 7,
        tick: 50,
        data: &[],
        world: Some(&view),
    };
    for enabled in [false, true] {
        let mut system = bloxgloom_lifecycle_fixture::system::definition();
        system.behavior = Arc::new(MotionOwner(enabled));
        system.read_radius_chunks = Some(0);
        let mut plan = system.behavior.plan(&context).unwrap();
        plan.moving_spawns.push(spawn());
        let result = prepare(&mut plan, &context, &job, &system);
        if enabled {
            result.unwrap();
            assert!(plan.moving_spawns.is_empty());
            let record = Record::decode(&plan.entity_spawns[0].state).unwrap();
            assert_eq!(record.state, vec![1]);
            assert_eq!(record.motion.velocity, [1.0, 0.0, 0.0]);
            assert_eq!(record.simulation_tick, 50);
        } else {
            assert!(result.is_err());
        }
    }
    let mut system = bloxgloom_lifecycle_fixture::system::definition();
    system.behavior = Arc::new(MotionOwner(true));
    system.read_radius_chunks = Some(0);
    let mut plan = system.behavior.plan(&context).unwrap();
    let mut outside = spawn();
    outside.position[0] = 32.0;
    plan.moving_spawns.push(outside);
    assert!(prepare(&mut plan, &context, &job, &system).is_err());
}
