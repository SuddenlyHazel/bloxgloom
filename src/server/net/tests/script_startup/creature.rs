//! Luau creature registration, server tick callback and inert client model.
use super::*;
use bloxgloom_host_api::entity::{self as api, World};

const REGISTER: &str = "return function(h) h.register_creature{key='demo:sproutling',module='demo:tick',schema=1,revision=1,max_state_bytes=8,initial_state='new',interval=1,body={half_width=0.25,height=0.7,speed=1.0},model={{min={-0.2,0.0,-0.2},max={0.2,0.7,0.2},color={0.2,0.8,0.3}}}} end";

fn package(fixture: &Fixture, source: &str) {
    fixture.package(
        "demo",
        "requires bloxgloom:content/v1\nrequires bloxgloom:mobile_entities/v1\nmodule tick tick.luau",
        source,
    );
    std::fs::write(
        fixture.0.join("packages/demo/tick.luau"),
        "return function(c) assert(c.data == 'new'); return 'done',10,nil,nil end",
    )
    .unwrap();
}

struct Flat;
impl World for Flat {
    fn solid(&self, _: [i32; 3]) -> Result<bool, api::Error> {
        Ok(false)
    }
    fn clear(&self, _: [f32; 3]) -> Result<bool, api::Error> {
        Ok(true)
    }
    fn grounded(&self, _: [f32; 3]) -> Result<bool, api::Error> {
        Ok(true)
    }
    fn walk_edge(&self, _: [f32; 3], _: [f32; 3]) -> Result<bool, api::Error> {
        Ok(true)
    }
    fn route(&self, _: [f32; 3], _: [i32; 2]) -> Result<api::Route, api::Error> {
        Ok(api::Route::Arrived)
    }
    fn advance(
        &self,
        position: [f32; 3],
        _: f32,
        _: Option<[f32; 3]>,
    ) -> Result<api::Movement, api::Error> {
        Ok(api::Movement {
            position,
            vertical_velocity: 0.0,
            grounded: true,
        })
    }
}

#[test]
fn luau_creature_negotiates_model_ticks_and_restarts() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    package(&fixture, REGISTER);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let entity = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
    let creature = catalog.mobile_entity(entity).unwrap();
    assert_eq!(creature.model.len(), 1);
    assert_eq!(creature.model[0].color, [0.2, 0.8, 0.3]);
    let initial = creature.behavior.initial();
    let context = api::Context {
        id: 37,
        tick: 1,
        next_tick: Some(1),
        position: [2.5, 80.0, 0.5],
        state: &initial,
        world: &Flat,
        neighbours: &[],
    };
    let plan = creature.behavior.tick(&context).unwrap();
    assert_eq!(plan.next_tick, Some(11));
    let bytes = creature
        .behavior
        .encode(plan.state.as_ref().unwrap())
        .unwrap();
    assert_eq!(&bytes[12..], b"done");
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x1f")
    );
    let fingerprint = catalog.fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x540).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:sproutling").unwrap();
        let model = client.mobile_entity(id).unwrap();
        assert_eq!(model.model[0].color, [0.2, 0.8, 0.3]);
        assert!(model.behavior.pose(&[0, 0, 0, 0, 1]).unwrap().grounded);
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/main.luau"),
        REGISTER.replace("0.8", "0.7"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_creature_rejects_invalid_declaration_before_save_and_caught_route_failure() {
    let missing = Fixture::new();
    missing.package(
        "demo",
        "requires bloxgloom:content/v1\nmodule tick tick.luau",
        REGISTER,
    );
    std::fs::write(
        missing.0.join("packages/demo/tick.luau"),
        "return function(c) return c.data,10,nil,nil end",
    )
    .unwrap();
    assert!(missing.open().is_err());
    assert!(!missing.0.join("save/content.map").exists());

    let invalid = Fixture::new();
    package(
        &invalid,
        &REGISTER.replace("color={0.2,0.8,0.3}", "color={0.2,1.8,0.3}"),
    );
    assert!(invalid.open().is_err());
    assert!(!invalid.0.join("save/content.map").exists());

    let fixture = Fixture::new();
    package(&fixture, REGISTER);
    std::fs::write(
        fixture.0.join("packages/demo/tick.luau"),
        "return function(c) pcall(function() for i=1,9 do c.route(0,0) end end); return 'bad',1,nil,nil end",
    ).unwrap();
    let state = fixture.open().unwrap();
    let catalog = state.world.catalog();
    let id = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
    let creature = catalog.mobile_entity(id).unwrap();
    let initial = creature.behavior.initial();
    let context = api::Context {
        id: 1,
        tick: 1,
        next_tick: Some(1),
        position: [2.5, 80.0, 0.5],
        state: &initial,
        world: &Flat,
        neighbours: &[],
    };
    assert!(creature.behavior.tick(&context).is_err());
    assert_eq!(&creature.behavior.encode(&initial).unwrap()[12..], b"new");
}

#[test]
fn luau_creature_interaction_and_animation_negotiate_and_keep_private_state() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = REGISTER.replace(
        "model={{",
        "interaction='pat',animation={stride_rate=7.0,idle_bob=0.02},model={{",
    );
    package(&fixture, &register);
    std::fs::write(
        fixture.0.join("packages/demo/tick.luau"),
        "return function(c) if c.event == 'interact' then assert(c.request == 'pat' and c.data == 'new'); return 'happy' end assert(c.event == 'tick'); return c.data,10,nil,nil end",
    ).unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
    let creature = catalog.mobile_entity(id).unwrap();
    assert_eq!(creature.interaction, b"pat");
    assert_eq!(creature.animation.stride_rate, 7.0);
    assert_eq!(creature.animation.idle_bob, 0.02);
    let initial = creature.behavior.initial();
    let interacted = creature.behavior.interact(&initial, b"pat").unwrap();
    assert_eq!(
        &creature.behavior.encode(&interacted).unwrap()[12..],
        b"happy"
    );
    assert_eq!(&creature.behavior.encode(&initial).unwrap()[12..], b"new");
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x21")
    );
    let fingerprint = catalog.fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x541).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:sproutling").unwrap();
        let descriptor = client.mobile_entity(id).unwrap();
        assert_eq!(descriptor.interaction, b"pat");
        assert_eq!(descriptor.animation.stride_rate, 7.0);
        assert_eq!(descriptor.animation.idle_bob, 0.02);
        assert!(
            descriptor
                .behavior
                .interact(&descriptor.behavior.initial(), b"pat")
                .is_err()
        );
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/main.luau"),
        register.replace("stride_rate=7.0", "stride_rate=8.0"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_creature_options_reject_invalid_bounds_before_save_creation() {
    for declaration in [
        REGISTER.replace("model={{", "animation={stride_rate=41},model={{"),
        REGISTER.replace(
            "model={{",
            &format!("interaction='{}',model={{{{", "x".repeat(129)),
        ),
    ] {
        let fixture = Fixture::new();
        package(&fixture, &declaration);
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/content.map").exists());
    }
}

#[test]
fn luau_creature_world_services_are_bounded_and_caught_errors_reject_tick() {
    for (source, accepted) in [
        (
            "return function(c) assert(c.solid(2,80,0) == false); assert(c.clear(2.5,80,0.5)); assert(c.grounded(2.5,80,0.5)); assert(c.walk_edge(2.5,80,0.5,3.5,80,0.5)); return 'ok',2,nil,nil end",
            true,
        ),
        (
            "return function(c) pcall(function() c.solid(1.5,80,0) end); return 'bad',2,nil,nil end",
            false,
        ),
        (
            "return function(c) pcall(function() for i=1,17 do c.clear(2.5,80,0.5) end end); return 'bad',2,nil,nil end",
            false,
        ),
    ] {
        let fixture = Fixture::new();
        package(&fixture, REGISTER);
        std::fs::write(fixture.0.join("packages/demo/tick.luau"), source).unwrap();
        let state = fixture.open().unwrap();
        let catalog = state.world.catalog();
        let id = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
        let creature = catalog.mobile_entity(id).unwrap();
        let initial = creature.behavior.initial();
        let context = api::Context {
            id: 1,
            tick: 1,
            next_tick: Some(1),
            position: [2.5, 80.0, 0.5],
            state: &initial,
            world: &Flat,
            neighbours: &[],
        };
        assert_eq!(creature.behavior.tick(&context).is_ok(), accepted);
        assert_eq!(&creature.behavior.encode(&initial).unwrap()[12..], b"new");
    }
}

#[test]
fn luau_creature_tick_can_request_bounded_self_spawn_and_despawn() {
    let fixture = Fixture::new();
    package(&fixture, REGISTER);
    std::fs::write(
        fixture.0.join("packages/demo/tick.luau"),
        "return function(c) return 'parent',2,nil,nil,{despawn=true,spawns={{c.position[1]+1,c.position[2],c.position[3]}}} end",
    ).unwrap();
    let state = fixture.open().unwrap();
    let catalog = state.world.catalog();
    let id = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
    let creature = catalog.mobile_entity(id).unwrap();
    let initial = creature.behavior.initial();
    let context = api::Context {
        id: 1,
        tick: 1,
        next_tick: Some(1),
        position: [2.5, 80.0, 0.5],
        state: &initial,
        world: &Flat,
        neighbours: &[],
    };
    let plan = creature.behavior.tick(&context).unwrap();
    assert!(plan.lifecycle.despawn);
    assert_eq!(plan.lifecycle.spawns.len(), 1);
    assert_eq!(plan.lifecycle.spawns[0].key, "demo:sproutling");
    assert_eq!(plan.lifecycle.spawns[0].position, [3.5, 80.0, 0.5]);
    assert_eq!(
        creature
            .behavior
            .encode(&plan.lifecycle.spawns[0].state)
            .unwrap(),
        creature.behavior.encode(&initial).unwrap()
    );
}

#[test]
fn luau_creature_rejects_invalid_lifecycle_before_movement_or_state_change() {
    for source in [
        "return function(c) return 'bad',2,nil,nil,{spawns={{c.position[1]+9,c.position[2],c.position[3]}}} end",
        "return function(c) return 'bad',2,nil,nil,{spawns={{1,2,3},{1,2,3},{1,2,3},{1,2,3},{1,2,3}}} end",
        "return function(c) return 'bad',2,nil,nil,{despawn=1} end",
    ] {
        let fixture = Fixture::new();
        package(&fixture, REGISTER);
        std::fs::write(fixture.0.join("packages/demo/tick.luau"), source).unwrap();
        let state = fixture.open().unwrap();
        let catalog = state.world.catalog();
        let id = catalog.entity_type_id_by_key("demo:sproutling").unwrap();
        let creature = catalog.mobile_entity(id).unwrap();
        let initial = creature.behavior.initial();
        let context = api::Context {
            id: 1,
            tick: 1,
            next_tick: Some(1),
            position: [2.5, 80.0, 0.5],
            state: &initial,
            world: &Flat,
            neighbours: &[],
        };
        assert!(creature.behavior.tick(&context).is_err());
        assert_eq!(&creature.behavior.encode(&initial).unwrap()[12..], b"new");
    }
}
