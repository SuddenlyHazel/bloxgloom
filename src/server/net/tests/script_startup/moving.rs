//! Production nonblocking listener, finite launch debit and durable reactions.
use super::*;
use crate::{inventory::Stack, server::entities::EntityId};
use bloxgloom_host_api::motion::{Pending, Record};

const PROFILE: u128 = 0x5c71;
mod load;
const REGISTER: &str = r#"return function(h)
    h.register_moving_entity{key='demo:projectile',module='demo:behavior',schema=1,revision=1,
        max_state_bytes=1,max_public_bytes=1,interval=1000,lifetime_ticks=1000,
        handles_impact=true,body={half_extents={0.12,0.12,0.12},max_speed=24,
            max_acceleration=32,gravity_scale=1,response='stop'},
        model={{min={-0.12,-0.12,-0.12},max={0.12,0.12,0.12},color={0.2,0.8,0.3}}}}
    h.register_action('demo:shift',1,'Launch','item','bloxgloom:stick','demo:action')
end"#;
const ACTION: &str = r#"return function(c,e)
    assert(c.take('player',e.slot,1) ~= nil)
    local mode = e.arguments:byte(1)
    local position = if mode == 1 then {0.5,79.5,0.5} else {2.5,80.6,0.5}
    c.spawn_moving_entity('demo:projectile',{position=position,velocity={8,-2,0},state=if mode == 3 then 'F' else 'N'})
    if mode == 2 then pcall(function() c.set_motion(1,1,{position={0,0,0}}) end) end
end"#;
const BEHAVIOR: &str = r#"return function(c,e)
    if e.kind == 'MovingTick' then return end
    assert(e.kind == 'MovingImpact')
    local state=c.entity_state(e.entity)
    if state == 'F' then
        assert(c.update_entity(e.entity,'I'))
        c.spawn_drop(6.5,80.5,0.5,'bloxgloom:stick',1,4294967295)
        error('deliberately unresolved impact after staging effects')
    end
    assert(state == 'N', 'impact effects repeated')
    assert(e.target.kind == 'Terrain' and e.normal[2] == 1)
    assert(c.update_entity(e.entity,'I'))
    local motion=c.motion(e.entity)
    assert(motion ~= nil)
    assert(c.set_motion(e.entity,motion.revision,{velocity={0,0,0}}))
    c.spawn_drop(6.5,80.5,0.5,'bloxgloom:stick',1,4294967295)
end"#;

fn package(fixture: &Fixture) {
    fixture.package("demo", "requires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:moving_entities/v1\nmodule action action.luau\nmodule behavior behavior.luau", REGISTER);
    let dir = fixture.0.join("packages/demo");
    std::fs::write(dir.join("action.luau"), ACTION).unwrap();
    std::fs::write(dir.join("behavior.luau"), BEHAVIOR).unwrap();
}
fn prepare(state: &mut State) {
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=9 {
        for z in -1..=1 {
            state.world.edit(x, 79, z, crate::world::STONE).unwrap();
            for y in 80..=84 {
                state.world.edit(x, y, z, crate::world::AIR).unwrap();
            }
        }
    }
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        state
            .world
            .catalog()
            .item_by_key("bloxgloom:stick")
            .unwrap(),
        4,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
}
fn records(state: &State) -> Vec<(EntityId, Record)> {
    let kind = state
        .world
        .catalog()
        .entity_type_id_by_key("demo:projectile")
        .unwrap();
    state
        .entities
        .query_mobile_aabb([-1.0, 78.0, -1.0], [10.0, 85.0, 2.0])
        .unwrap()
        .into_iter()
        .filter_map(|id| {
            let snapshot = state.entities.snapshot(id).unwrap();
            (snapshot.entity_type == kind).then(|| {
                (
                    id,
                    Record::decode(snapshot.private_payload.downcast_ref::<Vec<u8>>().unwrap())
                        .unwrap(),
                )
            })
        })
        .collect()
}

#[test]
fn moving_loopback_debits_once_sweeps_reacts_and_recovers_consumed_impact() {
    let fixture = Fixture::new();
    package(&fixture);
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let kind = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    let stick = catalog.item_by_key("bloxgloom:stick").unwrap();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, Arc::clone(&catalog));
        for mode in [1, 2] {
            let request = peer.request(mode);
            let (accepted, reason) = peer.send(&request);
            assert!(!accepted, "invalid launch {mode} accepted: {reason}");
            assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 4);
        }
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut impacted = false;
        let mut refunded = false;
        while !impacted || !refunded {
            match peer.read(deadline) {
                ServerMessage::Drops { items, .. } => {
                    refunded |= items
                        .iter()
                        .any(|item| item.item == stick && item.count == 1)
                }
                ServerMessage::WorldCommitPart(part) => {
                    for change in part.entities {
                        if let protocol::PublicEntityChange::Upsert(entity) = change
                            && entity.entity_type == kind
                        {
                            // Moving replicas carry their pose envelope plus only the public byte.
                            impacted |= entity.payload.last() == Some(&b'I');
                        }
                    }
                }
                ServerMessage::EntitySnapshotPage(page) => {
                    impacted |= page
                        .entities
                        .iter()
                        .any(|e| e.entity_type == kind && e.payload.last() == Some(&b'I'))
                }
                _ => {}
            }
        }
        assert!(peer.send(&request).0, "duplicate launch receipt rejected");
        assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 3);
    });
    let state = Box::new(fixture.open().unwrap());
    let saved = records(&state);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].1.state, b"I");
    assert!(saved[0].1.pending.is_none());
    assert!(saved[0].1.motion.revision > 0);
    let drops = crate::server::drops::nearby(&state.entities, [6.5, 80.5, 0.5]);
    assert_eq!(
        drops
            .iter()
            .filter(|drop| drop.item == stick)
            .map(|drop| u32::from(drop.count))
            .sum::<u32>(),
        1
    );
    let before = saved[0].1.remaining_ticks;
    let id = saved[0].0;
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut saw = false;
        while !saw {
            if let ServerMessage::EntitySnapshotPage(page) = peer.read(deadline) {
                saw = page
                    .entities
                    .iter()
                    .any(|e| e.id == id.get() && e.payload.last() == Some(&b'I'));
            }
        }
    });
    let state = fixture.open().unwrap();
    let saved = records(&state);
    assert_eq!(saved[0].0, id);
    assert_eq!(saved[0].1.state, b"I");
    assert!(saved[0].1.pending.is_none());
    assert!(saved[0].1.remaining_ticks <= before);
    assert_eq!(
        crate::server::drops::nearby(&state.entities, [6.5, 80.5, 0.5])
            .iter()
            .filter(|drop| drop.item == stick)
            .count(),
        1
    );
}

#[test]
fn moving_failed_impact_remains_durable_and_does_not_grant_partial_reward() {
    let fixture = Fixture::new();
    package(&fixture);
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let kind = catalog.entity_type_id_by_key("demo:projectile").unwrap();
    gameplay::serve(state, |address| {
        let mut peer = gameplay::Peer::connect(address, catalog);
        let request = peer.request(3);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::WorldCommitPart(part) = peer.read(deadline)
                && part.entities.iter().any(|change| {
                    matches!(change, protocol::PublicEntityChange::Upsert(e)
                        if e.entity_type == kind && e.motion_revision > 1
                            && bloxgloom_host_api::motion::Projection::decode(&e.payload)
                                .is_ok_and(|pose| pose.stopped))
                })
            {
                break;
            }
        }
    });
    let state = fixture.open().unwrap();
    let saved = records(&state);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].1.state, b"F");
    assert!(matches!(&saved[0].1.pending, Some(Pending::Impact(_))));
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
fn moving_seed_fixture_negotiates_models_and_client_sources_over_real_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/moving-projectiles/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&packages)
        .unwrap();
    let state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    );
    let catalog = state.world.catalog_arc();
    let fingerprint = catalog.fingerprint();
    for key in ["throw:seed_body", "throw:guided_body"] {
        let id = catalog.entity_type_id_by_key(key).unwrap();
        assert_eq!(catalog.moving_entity(id).unwrap().model.len(), 1);
    }
    assert!(state.client_bundle.as_ref().unwrap().bytes().len() > 1024);
    gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x540).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("throw:guided_body").unwrap();
        let declaration = client.moving_entity(id).unwrap();
        assert_eq!(declaration.body.gravity_scale, 0.0);
        assert_eq!(declaration.model[0].color, [0.2, 0.5, 1.0]);
    });
}
