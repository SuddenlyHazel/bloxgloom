//! Real listener validates script steering, publication, finite debit and restart.
use super::*;
use bloxgloom_host_api::motion::Projection;
const REGISTER: &str = r#"return function(h)
 h.register_moving_entity{key='demo:projectile',module='demo:behavior',schema=1,revision=1,max_state_bytes=1,max_public_bytes=1,interval=1000,lifetime_ticks=1000,
  body={half_extents={0.3,0.15,0.1},max_speed=8,max_acceleration=16,gravity_scale=0,response='slide'},
  physics={linear_damping=2,angular_damping=0,friction=0.5,max_angular_speed=4},
  model={{min={-0.3,-0.15,-0.1},max={0.3,0.15,0.1},color={0.4,0.7,0.9}}}}
 h.register_action('demo:shift',1,'Launch or spin','item','bloxgloom:stick','demo:action')
end"#;
const ACTION: &str = r#"return function(c,e)
 local mode=e.arguments:byte(1)
 if mode==0 then
  assert(c.take('player',e.slot,1))
  c.spawn_moving_entity('demo:projectile',{position={2.5,82.5,0.5},velocity={1,0,0},angular_velocity={0,2,0},state='S'})
 else
  for _,entity in c.nearby_entities(2.5,82.5,0.5,8) do
   if entity.entity_type=='demo:projectile' then
    local motion=c.motion(entity.id);assert(motion)
    assert(not pcall(function() motion.angular_velocity[2]=9 end))
    if mode==1 then
     assert(c.set_motion(entity.id,motion.revision,{angular_velocity={0,4,0}}))
     assert(c.update_entity(entity.id,'R'))
    elseif mode==2 then
     pcall(function() c.set_motion(entity.id,motion.revision,{angular_velocity={0,9,0}}) end)
     assert(c.update_entity(entity.id,'X'))
    else
     pcall(function() c.set_motion(entity.id,motion.revision,{orientation={0,0,1,0}}) end)
     assert(c.update_entity(entity.id,'X'))
    end
    return
   end
  end
  error('rigid body unavailable')
 end
end"#;
fn package(fixture: &Fixture) {
    fixture.package("demo","requires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:moving_entities/v1\nmodule action action.luau\nmodule behavior behavior.luau",REGISTER);
    let directory = fixture.0.join("packages/demo");
    std::fs::write(directory.join("action.luau"), ACTION).unwrap();
    std::fs::write(directory.join("behavior.luau"), "return function(c,e) end").unwrap();
}
fn wait_public(
    peer: &mut gameplay::Peer,
    kind: crate::content::EntityTypeId,
    tag: u8,
) -> Projection {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entities = match peer.read(deadline) {
            ServerMessage::WorldCommitPart(part) => part
                .entities
                .into_iter()
                .filter_map(|change| match change {
                    protocol::PublicEntityChange::Upsert(entity) => Some(entity),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            ServerMessage::EntitySnapshotPage(page) => page.entities,
            _ => continue,
        };
        for entity in entities {
            if entity.entity_type == kind {
                let projection = Projection::decode(&entity.payload).unwrap();
                if projection.data == [tag]
                    && projection.motion.revision > 0
                    && projection.motion.orientation != [0.0, 0.0, 0.0, 1.0]
                {
                    return projection;
                }
            }
        }
    }
}
#[test]
fn moving_rigid_real_tcp_steering_is_owned_bounded_replicated_and_saved() {
    let fixture = Fixture::new();
    package(&fixture);
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let kind = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        let request = peer.request(0);
        let result = peer.send(&request);
        assert!(result.0, "{}", result.1);
        let projection = wait_public(&mut peer, kind, b'S');
        assert_eq!(projection.motion.angular_velocity, [0.0, 2.0, 0.0]);
        assert!(projection.motion.velocity[0] < 1.0);
    });
    let state = Box::new(fixture.open().unwrap());
    let saved = records(&state);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].1.motion.angular_velocity, [0.0, 2.0, 0.0]);
    assert_ne!(saved[0].1.motion.orientation, [0.0, 0.0, 0.0, 1.0]);
    let before = saved[0].1.clone();
    let catalog = state.world.catalog_arc();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        for mode in [2, 3] {
            let request = peer.request(mode);
            assert!(!peer.send(&request).0);
        }
        let request = peer.request(1);
        let result = peer.send(&request);
        assert!(result.0, "{}", result.1);
        let projection = wait_public(&mut peer, kind, b'R');
        assert_eq!(projection.motion.angular_velocity, [0.0, 4.0, 0.0]);
        assert!(projection.motion.revision > before.motion.revision);
    });
    let state = Box::new(fixture.open().unwrap());
    let saved = records(&state);
    assert_eq!(saved[0].1.state, b"R");
    assert_eq!(saved[0].1.motion.angular_velocity, [0.0, 4.0, 0.0]);
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        3
    );
}
