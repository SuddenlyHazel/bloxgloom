//! Exact dynamic impact targets, bounded source exclusions and durable expiry.
use super::*;
use bloxgloom_host_api::motion::{Projection, SpawnReceipt};
const REGISTER: &str = r#"return function(h)
 h.register_creature{key='demo:target',module='demo:creature',schema=1,revision=1,max_state_bytes=1,initial_state='T',interval=1000,body={half_width=0.25,height=1,speed=0},model={{min={-0.2,0,-0.2},max={0.2,1,0.2},color={0.3,0.8,0.2}}}}
 h.register_moving_entity{key='demo:projectile',module='demo:behavior',schema=1,revision=1,max_state_bytes=1,max_public_bytes=1,interval=1000,lifetime_ticks=1000,source_exclusion_ticks=20,handles_impact=true,body={half_extents={0.1,0.1,0.1},max_speed=24,max_acceleration=32,gravity_scale=0,response='stop',collisions={terrain=false,players=true,creatures=true}}}
 h.register_action('demo:shift',1,'Launch','item','bloxgloom:stick','demo:action')
end"#;
const ACTION: &str = r#"return function(c,e)
 local mode=e.arguments:byte(1)
 if mode==3 then c.admin_spawn('demo:target');return end
 local me=c.player_by_profile(c.player_profile)
 local source=if mode==1 then me.entity else nil
 local position={2.5,80.5,0.5}
 if mode==2 then
  local found=false
  for _,target in c.nearby_entities(0.5,80.5,0.5,8) do
   if target.entity_type=='demo:target' then position={target.position[1]+2,target.position[2]+0.5,target.position[3]};found=true end
  end
  assert(found)
 end
 local ref=c.spawn_moving_entity('demo:projectile',{position=position,velocity={-8,0,0},state=if mode==2 then 'C' else 'P',source=source})
 assert(ref.ordinal==0)
end"#;
const BEHAVIOR: &str = r#"return function(c,e)
 if e.kind=='MovingTick' then return end
 assert(e.kind=='MovingImpact' and e.target.kind=='Entity')
 local state=c.entity_state(e.entity)
 if state=='P' then assert(e.target.entity==c.players()[1].entity)
 else
  assert(state=='C')
  local target=c.entity(e.target.entity)
  assert(target and target.entity_type=='demo:target')
 end
 local contact=c.motion_contact(e.entity)
 assert(contact and contact.target.entity==e.target.entity and contact.motion_revision==c.motion(e.entity).revision)
 assert(contact.normal[1]==1 and not pcall(function() contact.normal[1]=0 end))
 assert(c.update_entity(e.entity,'I'))
end"#;
fn install(fixture: &Fixture, register: &str, action: &str, behavior: &str) {
    fixture.package("demo","requires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:players/v1\nrequires bloxgloom:mobile_entities/v1\nrequires bloxgloom:moving_entities/v1\nmodule action action.luau\nmodule behavior behavior.luau\nmodule creature creature.luau\nmodule policy policy.luau",register);
    let dir = fixture.0.join("packages/demo");
    std::fs::write(dir.join("action.luau"), action).unwrap();
    std::fs::write(dir.join("behavior.luau"), behavior).unwrap();
    std::fs::write(dir.join("policy.luau"), "return function() end").unwrap();
    std::fs::write(
        dir.join("creature.luau"),
        "return function(c) return c.data,1000,nil,nil end",
    )
    .unwrap();
}
fn launch(peer: &mut gameplay::Peer, mode: u8) -> (ClientMessage, Vec<SpawnReceipt>) {
    let request = peer.request(mode);
    peer.write(&request);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut mapped = None;
    loop {
        match peer.read(deadline) {
            ServerMessage::ActionSpawned { spawned, .. } => {
                assert!(mapped.is_none());
                mapped = Some(spawned);
            }
            ServerMessage::ActionResult {
                accepted, reason, ..
            } => {
                assert!(accepted, "{reason}");
                break;
            }
            _ => {}
        }
    }
    let mapped = mapped.expect("committed mapping must precede terminal result");
    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].ordinal, 0);
    (request, mapped)
}
fn wait_pose(peer: &mut gameplay::Peer, id: u64, predicate: impl Fn(&Projection) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entities = match peer.read(deadline) {
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
        if entities.into_iter().any(|e| {
            e.id == id && Projection::decode(&e.payload).is_ok_and(|pose| predicate(&pose))
        }) {
            return;
        }
    }
}
#[test]
fn moving_real_listener_dynamic_targets_source_exclusion_and_exact_receipt_replay() {
    for mode in [0, 1, 2] {
        let fixture = Fixture::new();
        install(&fixture, REGISTER, ACTION, BEHAVIOR);
        let mut state = Box::new(fixture.open().unwrap());
        prepare(&mut state);
        let catalog = state.world.catalog_arc();
        if mode == 2 {
            state.admin_profile = Some(PROFILE);
        }
        gameplay::serve(state, |address| {
            let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
            if mode == 2 {
                let seed = peer.request(3);
                let (accepted, reason) = peer.send(&seed);
                assert!(accepted, "{reason}");
            }
            let (request, mapping) = launch(&mut peer, mode);
            if mode == 1 {
                wait_pose(&mut peer, mapping[0].entity, |pose| {
                    pose.motion.position[0] < -0.6 && pose.data == b"P"
                });
            } else {
                wait_pose(&mut peer, mapping[0].entity, |pose| pose.data == b"I");
            }
            peer.write(&request);
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut replay = None;
            loop {
                match peer.read(deadline) {
                    ServerMessage::ActionSpawned { spawned, .. } => replay = Some(spawned),
                    ServerMessage::ActionResult { accepted, .. } => {
                        assert!(accepted);
                        assert_eq!(replay.as_ref(), Some(&mapping));
                        break;
                    }
                    _ => {}
                }
            }
        });
        let state = fixture.open().unwrap();
        let kind = state
            .world
            .catalog()
            .entity_type_id_by_key("demo:projectile")
            .unwrap();
        let saved = state
            .entities
            .record_values()
            .filter(|record| record.entity_type == kind)
            .map(|record| {
                Record::decode(record.payload.downcast_ref::<Vec<u8>>().unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].state, if mode == 1 { b"P" } else { b"I" });
    }
}
#[test]
fn moving_real_listener_expiry_effect_is_once_and_replay_preserves_historical_id() {
    let fixture = Fixture::new();
    let register = REGISTER
        .replace("lifetime_ticks=1000", "lifetime_ticks=4")
        .replace(
            "handles_impact=true",
            "handles_impact=true,handles_expiry=true",
        )
        .replace(
            "end",
            "h.register_player_lifecycle('demo:reward',1,16,'0','demo:policy') end",
        );
    install(
        &fixture,
        &register,
        "return function(c,e) c.spawn_moving_entity('demo:projectile',{position={2.5,80.5,0.5},state='X'}) end",
        r#"return function(c,e)
  if e.kind=='MovingTick' then return end
  assert(e.kind=='MovingExpiry' and e.reason=='Lifetime')
  assert(c.entity_state(e.entity)=='X')
  local me=c.players()[1]
  assert(c.profile_state('demo:reward',me.profile).state=='0')
  c.set_profile_state('demo:reward',me.profile,'R','rewarded')
  assert(c.give(me.profile,{item='bloxgloom:stick',count=1}))
  c.spawn_drop(6.5,80.5,0.5,'bloxgloom:stick',1,4294967295)
 end"#,
    );
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
    let mut historical = None;
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
        let (request, mapping) = launch(&mut peer, 0);
        historical = Some((request.clone(), mapping.clone()));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::Drops { items, .. } = peer.read(deadline)
                && items.iter().any(|item| item.item == stick)
            {
                break;
            }
        }
        peer.write(&request);
        let mut replay = None;
        loop {
            match peer.read(deadline) {
                ServerMessage::ActionSpawned { spawned, .. } => replay = Some(spawned),
                ServerMessage::ActionResult {
                    accepted, reason, ..
                } => {
                    assert!(accepted, "{reason}");
                    assert_eq!(replay.as_ref(), Some(&mapping));
                    break;
                }
                _ => {}
            }
        }
    });
    let state = Box::new(fixture.open().unwrap());
    assert!(records(&state).is_empty());
    let (_, cell) = state
        .system_runtime
        .owner_snapshot(
            &crate::server::registry::SystemId::new("demo:reward").unwrap(),
            crate::server::parallel::OwnerKey::Profile(PROFILE),
        )
        .unwrap();
    assert_eq!(
        cell.get::<bloxgloom_host_api::players::State>()
            .unwrap()
            .data,
        b"R"
    );
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        5
    );
    assert_eq!(
        crate::server::drops::nearby(&state.entities, [6.5, 80.5, 0.5])
            .iter()
            .filter(|drop| drop.item == stick)
            .count(),
        1
    );
    // Reconnecting grants a new durable epoch; stale requests cannot relaunch.
    let (request, _) = historical.unwrap();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        peer.write(&request);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut replay = None;
        loop {
            match peer.read(deadline) {
                ServerMessage::ActionSpawned { spawned, .. } => replay = Some(spawned),
                ServerMessage::ActionResult {
                    accepted, reason, ..
                } => {
                    assert!(!accepted && reason.contains("session"), "{reason}");
                    assert!(replay.is_none());
                    break;
                }
                _ => {}
            }
        }
    });
    assert!(records(&fixture.open().unwrap()).is_empty());
}

#[test]
fn moving_real_listener_admin_cancel_rejects_nonadmin_and_clears_stuck_reaction() {
    let fixture = Fixture::new();
    super::package(&fixture);
    let action = super::ACTION.replace(
        "    assert(c.take('player',e.slot,1) ~= nil)",
        r#"    if e.arguments:byte(1)==4 then
        local found=false
        for _,entity in c.nearby_entities(2.5,80.5,0.5,8) do
            if entity.entity_type=='demo:projectile' then
                assert(c.cancel_moving_entity(entity.id));found=true
            end
        end
        assert(found);return
    end
    assert(c.take('player',e.slot,1) ~= nil)"#,
    );
    std::fs::write(fixture.0.join("packages/demo/action.luau"), action).unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
        let (_, mapping) = launch(&mut peer, 3);
        wait_pose(&mut peer, mapping[0].entity, |pose| {
            pose.stopped && pose.motion.revision >= 2
        });
    });
    let state = Box::new(fixture.open().unwrap());
    assert!(matches!(
        records(&state)[0].1.pending,
        Some(Pending::Impact(_))
    ));
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
        let request = peer.request(4);
        let (accepted, reason) = peer.send(&request);
        assert!(!accepted && reason.contains("admin"), "{reason}");
    });
    let mut state = Box::new(fixture.open().unwrap());
    assert!(matches!(
        records(&state)[0].1.pending,
        Some(Pending::Impact(_))
    ));
    state.admin_profile = Some(PROFILE);
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        let request = peer.request(4);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
    });
    let state = fixture.open().unwrap();
    assert!(records(&state).is_empty());
    assert!(crate::server::drops::nearby(&state.entities, [6.5, 80.5, 0.5]).is_empty());
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        3
    );
}

#[test]
fn moving_failed_expiry_keeps_profile_inventory_and_drop_rewards_atomic() {
    let fixture = Fixture::new();
    let register = REGISTER
        .replace("lifetime_ticks=1000", "lifetime_ticks=4")
        .replace(
            "handles_impact=true",
            "handles_impact=true,handles_expiry=true",
        )
        .replace(
            "end",
            "h.register_player_lifecycle('demo:reward',1,16,'0','demo:policy') end",
        );
    install(
        &fixture,
        &register,
        "return function(c,e) c.spawn_moving_entity('demo:projectile',{position={2.5,80.5,0.5},state='F'}) end",
        r#"return function(c,e)
            if e.kind=='MovingTick' then return end
            assert(e.kind=='MovingExpiry')
            local me=c.players()[1]
            c.set_profile_state('demo:reward',me.profile,'R','rewarded')
            assert(c.give(me.profile,{item='bloxgloom:stick',count=1}))
            c.spawn_drop(6.5,80.5,0.5,'bloxgloom:stick',1,4294967295)
            error('rollback profile and inventory rewards')
        end"#,
    );
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        let (_, mapping) = launch(&mut peer, 0);
        wait_pose(&mut peer, mapping[0].entity, |pose| {
            pose.stopped && pose.motion.revision >= 2
        });
    });
    let state = fixture.open().unwrap();
    assert!(matches!(
        records(&state)[0].1.pending,
        Some(Pending::Expiry { .. })
    ));
    let (_, cell) = state
        .system_runtime
        .owner_snapshot(
            &crate::server::registry::SystemId::new("demo:reward").unwrap(),
            crate::server::parallel::OwnerKey::Profile(PROFILE),
        )
        .unwrap();
    assert_eq!(
        cell.get::<bloxgloom_host_api::players::State>()
            .unwrap()
            .data,
        b"0"
    );
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        4
    );
    assert!(crate::server::drops::nearby(&state.entities, [6.5, 80.5, 0.5]).is_empty());
}
