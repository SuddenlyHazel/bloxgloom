//! Persisted bodies pause without interest, then collide with authoritative cold terrain.
use super::*;
use bloxgloom_host_api::motion::{Projection, Target};
const REGISTER: &str = r#"return function(h)
 h.register_moving_entity{key='demo:projectile',module='demo:behavior',schema=1,revision=1,max_state_bytes=1,max_public_bytes=1,interval=1000,lifetime_ticks=1000,handles_impact=true,body={half_extents={0.1,0.1,0.1},max_speed=32,max_acceleration=32,gravity_scale=0,response='stop'}}
 h.register_action('demo:shift',1,'Launch or steer','item','bloxgloom:stick','demo:action')
end"#;
const ACTION: &str = r#"return function(c,e)
 if e.arguments:byte(1)==0 then
  c.spawn_moving_entity('demo:projectile',{position={15.5,80.5,0.5},state='S'})
 else
  local found=false
  for _,entity in c.nearby_entities(15.5,80.5,0.5,2) do
   if entity.entity_type=='demo:projectile' then
    local motion=c.motion(entity.id);assert(motion)
    assert(c.set_motion(entity.id,motion.revision,{velocity={8,0,0}}));found=true
   end
  end
  assert(found)
 end
end"#;
const BEHAVIOR: &str = r#"return function(c,e)
 if e.kind=='MovingTick' then return end
 assert(e.kind=='MovingImpact' and e.target.kind=='Terrain')
 assert(e.target.cell[1]==16 and e.normal[1]==-1)
 assert(c.entity_state(e.entity)=='S')
 assert(c.update_entity(e.entity,'I'))
end"#;
fn body(state: &State, id: EntityId) -> Record {
    let snapshot = state.entities.snapshot(id).unwrap();
    Record::decode(snapshot.private_payload.downcast_ref::<Vec<u8>>().unwrap()).unwrap()
}
#[test]
fn moving_real_listener_cold_terrain_and_dormancy_preserve_then_resume_saved_motion() {
    let fixture = Fixture::new();
    fixture.package("demo","requires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:moving_entities/v1\nmodule action action.luau\nmodule behavior behavior.luau",REGISTER);
    let dir = fixture.0.join("packages/demo");
    std::fs::write(dir.join("action.luau"), ACTION).unwrap();
    std::fs::write(dir.join("behavior.luau"), BEHAVIOR).unwrap();
    let id = launch_fixture(&fixture);
    let baseline = saved_body(&fixture, id);
    assert_eq!(baseline.motion.position, [15.5, 80.5, 0.5]);
    dormant_fixture(&fixture);
    cold_resume_fixture(&fixture, id, &baseline);
    verify_fixture(&fixture, id);
}

// Keep the large server State constructor out of every phase's caller frame.
// Production owns one boxed State; the test likewise keeps only heap pointers.
#[inline(never)]
fn open_boxed(fixture: &Fixture) -> Box<State> {
    Box::new(fixture.open().unwrap())
}
#[inline(never)]
fn saved_body(fixture: &Fixture, id: EntityId) -> Record {
    body(&open_boxed(fixture), id)
}
#[inline(never)]
fn launch_fixture(fixture: &Fixture) -> EntityId {
    let mut state = open_boxed(fixture);
    prepare(&mut state);
    for x in 10..=16 {
        for z in -1..=1 {
            state.world.edit(x, 79, z, crate::world::STONE).unwrap();
            for y in 80..=84 {
                state.world.edit(x, y, z, crate::world::AIR).unwrap();
            }
        }
    }
    state.world.edit(16, 80, 0, crate::world::STONE).unwrap();
    state
        .position_store
        .save(PROFILE, [14.5, 80.0, 0.5])
        .unwrap();
    let catalog = state.world.catalog_arc();
    let mut id = None;
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
        let request = peer.request(0);
        peer.write(&request);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match peer.read(deadline) {
                ServerMessage::ActionSpawned { spawned, .. } => {
                    assert_eq!(spawned.len(), 1);
                    id = EntityId::new(spawned[0].entity);
                }
                ServerMessage::ActionResult {
                    accepted, reason, ..
                } => {
                    assert!(accepted, "{reason}");
                    assert!(id.is_some());
                    break;
                }
                _ => {}
            }
        }
    });
    id.unwrap()
}
#[inline(never)]
fn dormant_fixture(fixture: &Fixture) {
    // The actual listener advances wall time with no joined client.
    gameplay::serve(open_boxed(fixture), |_| {
        std::thread::sleep(Duration::from_millis(200))
    });
}
#[inline(never)]
fn cold_resume_fixture(fixture: &Fixture, id: EntityId, baseline: &Record) {
    let mut state = open_boxed(fixture);
    assert_eq!(body(&state, id), *baseline);
    state
        .world
        .reset_cache_for_test(crate::server::SERVER_CHUNK_CACHE);
    assert_eq!(state.world.cached_block(15, 80, 0), None);
    assert_eq!(state.world.cached_block(16, 80, 0), None);
    let error = match crate::server::entities::motion::plan(
        &mut state,
        id,
        baseline.simulation_tick.saturating_add(500),
    ) {
        Err(error) => error,
        Ok(_) => panic!("cold/dormant motion unexpectedly prepared a commit"),
    };
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(
        body(&state, id),
        *baseline,
        "cold/dormant planning must not advance a private record"
    );
    let catalog = state.world.catalog_arc();
    let kind = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    // Rejoin cold: only ordinary loader/streaming admission installs terrain. No
    // test-side fallback or synchronous get_block makes the wall authoritative.
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        let request = peer.request(1);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let updates = match peer.read(deadline) {
                ServerMessage::WorldCommitPart(part) => part
                    .entities
                    .into_iter()
                    .filter_map(|change| match change {
                        protocol::PublicEntityChange::Upsert(e) => Some(e),
                        _ => None,
                    })
                    .collect(),
                ServerMessage::EntitySnapshotPage(page) => page.entities,
                _ => Vec::new(),
            };
            let mut impacted = false;
            for entity in updates {
                if entity.id == id.get() && entity.entity_type == kind {
                    let pose = Projection::decode(&entity.payload).unwrap();
                    assert!(
                        pose.motion.position[0] <= 15.9001,
                        "a missing destination must never be treated as air"
                    );
                    impacted |= pose.data == b"I";
                }
            }
            if impacted {
                break;
            }
        }
    });
}
#[inline(never)]
fn verify_fixture(fixture: &Fixture, id: EntityId) {
    let mut state = open_boxed(fixture);
    let saved = body(&state, id);
    assert_eq!(saved.state, b"I");
    assert!(saved.pending.is_none());
    assert_eq!(saved.motion.velocity, [0.0; 3]);
    assert!(matches!(
        saved.contact,
        Some(Target::Terrain {
            cell: [16, 80, 0],
            ..
        })
    ));
    assert_eq!(
        state.world.get_block(16, 80, 0).unwrap(),
        crate::world::STONE
    );
}
