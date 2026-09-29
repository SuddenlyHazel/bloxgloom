//! New event owners through live commands, scheduled dispatch and WAL recovery.
use super::*;
use crate::server::{
    durable::{CommitBarrier, DurableRequest, complete_barrier, process_durable_actions},
    simulation::TickId,
};
use crate::world::STONE;
use bloxgloom_host_api::{Extension, Registrar, RegistrationError, gameplay as api};

const BLOCK_REGISTER: &str = r#"return function(h)
    h.register_handler('demo:removed',1,'BlockRemoved','bloxgloom:sand','demo:action')
    h.register_handler('demo:placed',1,'BlockPlaced','bloxgloom:sand','demo:action')
    h.register_handler('demo:neighbor',1,'NeighborChanged','bloxgloom:glowstone','demo:action')
end"#;
const BLOCK_SOURCE: &str = r#"return function(c,e)
    assert(not pcall(function() e.cell[1] = 99 end))
    assert(not pcall(function() e.previous.state = 'forged' end))
    assert(os == nil and print == nil and require == nil)
    if e.kind == 'BlockRemoved' then
        assert(e.previous.block_type == 'bloxgloom:sand' and e.cause == 'Break')
        assert(type(e.random_lo) == 'number' and type(e.random_hi) == 'number')
        assert(c.block(e.cell[1],80,0).state == 'bloxgloom:air')
        c.spawn_drop(8.5,80.5,0.5,'bloxgloom:seeds',2,4294967295)
        if e.cell[1] == 3 then
            c.set_block(4,80,0,'bloxgloom:stone')
            pcall(function() c.spawn_drop(8,80,0,'bloxgloom:seeds',129,0) end)
        end
    elseif e.kind == 'BlockPlaced' then
        assert(e.previous.state == 'bloxgloom:air' and e.placed.state == 'bloxgloom:sand')
        assert(c.block(e.cell[1],80,0).state == e.placed.state)
        c.spawn_drop(8.5,80.5,0.5,'bloxgloom:stick',1,4294967295)
    elseif e.kind == 'NeighborChanged' then
        assert(e.changed[1] == 2 and e.changed[2] == 80 and e.changed[3] == 0)
        assert(e.previous.state == 'bloxgloom:sand' and e.current.state == 'bloxgloom:air')
        c.set_block(e.cell[1],e.cell[2],e.cell[3],'bloxgloom:stone')
    else error('wrong event') end
end"#;

#[test]
fn entity_target_action_is_discovered_in_verified_session_catalog() {
    use bloxgloom_host_api::actions::Target;
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.action(
        "return function(h) h.register_entity('demo:marker',1,8,8,nil); h.register_action('demo:inspect',1,'Inspect','entity','demo:marker','demo:action') end",
        "return function(c,e) assert(e.kind == 'ActionRequested' and e.entity_lo ~= nil) end",
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    assert_eq!(
        catalog
            .discover_actions(&Target::Entity("demo:marker".into()))
            .count(),
        1
    );
    serve(state, |address| {
        let joined = crate::client::connect_catalog_probe(&address.to_string(), 0xa4).unwrap();
        assert_eq!(joined.fingerprint(), catalog.fingerprint());
        assert_eq!(
            joined
                .discover_actions(&Target::Entity("demo:marker".into()))
                .count(),
            1
        );
    });
}

fn prepare_player(state: &mut State) {
    state.spawn_anchor = [0.5, 80.0, 0.5];
    state.world.edit(0, 79, 0, STONE).unwrap();
    state.world.edit(0, 80, 0, AIR).unwrap();
    state.world.edit(0, 81, 0, AIR).unwrap();
}

#[test]
fn luau_decisions_block_events_caught_error_rollback_and_restart() {
    let fixture = Fixture::new();
    fixture.action(BLOCK_REGISTER, BLOCK_SOURCE);
    let mut state = Box::new(fixture.open().unwrap());
    prepare_player(&mut state);
    for (x, z, block) in [(2, 0, SAND), (3, 0, SAND), (4, 0, AIR), (2, 1, GLOWSTONE)] {
        state.world.edit(x, 80, z, block).unwrap();
    }
    let catalog = state.world.catalog_arc();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:sand").unwrap(),
        2,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for (x, block, accepted) in [(3, AIR, false), (2, AIR, true), (2, SAND, true)] {
            let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
            peer.sequence += 1;
            let result = peer.send(&ClientMessage::Edit {
                action_id,
                x,
                y: 80,
                z: 0,
                block,
                slot: 0,
            });
            assert_eq!(result.0, accepted, "{}", result.1);
        }
        peer.inventory_at(1);
    });
    let mut state = fixture.open().unwrap();
    assert_eq!(state.world.get_block(2, 80, 0).unwrap(), SAND);
    assert_eq!(state.world.get_block(3, 80, 0).unwrap(), SAND);
    assert_eq!(state.world.get_block(4, 80, 0).unwrap(), AIR);
    assert_eq!(state.world.get_block(2, 80, 1).unwrap(), STONE);
    let drops = crate::server::drops::nearby(&state.entities, [8.5, 80.5, 0.5]);
    let scripted: Vec<_> = drops
        .iter()
        .filter(|drop| {
            let key = &state.world.catalog().item(drop.item).unwrap().key;
            key.as_ref() == "bloxgloom:seeds" || key.as_ref() == "bloxgloom:stick"
        })
        .collect();
    assert_eq!(scripted.len(), 2, "caught error published a drop");
    assert_eq!(scripted.iter().map(|drop| drop.count).sum::<u16>(), 3);
    // The neighbor's glowstone -> stone effect still uses native WorldEdit
    // removal policy; the targeted sand handler replaces, not adds to, harvest.
    assert_eq!(drops.len(), 3);
    assert!(drops.iter().any(|drop| {
        state.world.catalog().item(drop.item).unwrap().key.as_ref() == "bloxgloom:glowstone"
            && drop.count == 1
    }));
}

struct Schemas;
struct ByteState;
impl api::EntityState for ByteState {
    fn validate(&self, bytes: &[u8]) -> Result<(), RegistrationError> {
        if bytes.len() != 1 || !(1..=4).contains(&bytes[0]) {
            return Err(RegistrationError("expected state byte 1..4".into()));
        }
        Ok(())
    }
    fn public(&self, bytes: &[u8]) -> Result<Vec<u8>, RegistrationError> {
        self.validate(bytes)?;
        Ok(bytes.to_vec())
    }
}
impl Extension for Schemas {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        for key in ["demo:marker", "foreign:marker"] {
            registrar.gameplay_entity(api::EntityDefinition {
                key: key.into(),
                schema_version: 1,
                schema_fingerprint: 123,
                max_state_bytes: 1,
                initial_delay_ticks: (key == "demo:marker").then_some(100000),
                state: Arc::new(ByteState),
            })?;
        }
        Ok(())
    }
}

impl Fixture {
    fn open_decisions(&self) -> io::Result<Box<State>> {
        // Scheduled schemas and decision owners must resolve in one public
        // bundle; no intermediate catalog may contain a handlerless timer.
        let scripts =
            crate::server::script::startup::Declarations::discover(&self.0.join("packages"))?;
        let startup = ServerStartup::new(Arc::new(Catalog::builtins())).with_extension(
            &bloxgloom_host_api::composition::Bundle(&[&Schemas, &scripts]),
        )?;
        crate::server::server_state_with_startup(7, self.0.join("save"), 2, startup).map(Box::new)
    }
}

const ENTITY_REGISTER: &str = r#"return function(h)
    h.register_action('demo:shift',1,'Spawn','item','bloxgloom:stick','demo:action')
    h.register_handler('demo:tick',1,'EntityTick','demo:marker','demo:action')
end"#;
const ENTITY_SOURCE: &str = r#"
local calls = 0
return function(c,e)
    calls += 1; assert(calls == 1)
    if e.kind == 'ActionRequested' then
        c.spawn_entity('demo:marker',6.5,80.5,0.5,string.char(1))
        c.spawn_entity('demo:marker',7.5,80.5,0.5,string.char(4))
        local mode = string.byte(e.arguments,1)
        if mode == 1 then pcall(function() c.spawn_entity('foreign:marker',8,80,0,string.char(1)) end) end
        if mode == 2 then pcall(function() c.spawn_entity('demo:marker',8,80,0,string.char(5)) end) end
        if mode == 3 then pcall(function() c.entity_state(0,0) end) end
        if mode == 4 then pcall(function() c.schedule_entity(1.5,0,1) end) end
        if mode == 5 then pcall(function() c.spawn_entity('demo:marker',8,80,0,string.rep('x',65536)) end) end
        if mode == 6 then pcall(function() c.nearby_entities(6,80,0,17) end) end
    else
        assert(e.kind == 'EntityTick' and e.tick == c.tick)
        assert(e.entity ~= nil and e.position[2] == 80.5)
        local first = c.random(6,80,0,5)
        local again = c.random(6,80,0,5)
        assert(first == again and first >= 0 and first < 1)
        local entity = c.entity(e.entity)
        assert(entity.entity_type == 'demo:marker' and entity.data == c.entity_state(e.entity))
        assert(entity.position[2] == 80.5 and entity.anchor == nil)
        assert(not pcall(function() entity.data = 'forged' end))
        assert(not pcall(function() entity.position[1] = 999 end))
        local nearby = c.nearby_entities(7,80.5,0.5,2)
        assert(#nearby >= 2 and not pcall(function() nearby[1] = nil end))
        local found = false
        for _, candidate in nearby do
            if candidate.id == e.entity then found = true end
        end
        assert(found)
        assert(c.anchored_entity_at(6,80,0) == nil)
        local n = string.byte(c.entity_state(e.entity),1)
        if n == 4 then
            assert(c.remove_entity(e.entity))
            assert(c.entity_state(e.entity) == nil)
            assert(c.entity(e.entity) == nil)
        else
            assert(c.update_entity(e.entity,string.char(n+1)))
            assert(c.entity_state(e.entity) == string.char(n+1))
            assert(c.entity(e.entity).data == string.char(n+1))
            if n == 1 then
                assert(c.schedule_entity(e.entity,2))
                c.spawn_drop(8.5,80.5,0.5,'bloxgloom:seeds',1,4294967295)
                local mode = c.block(5,80,0).state
                if mode == 'bloxgloom:glowstone' then
                    pcall(function() c.update_entity(e.entity,string.char(5)) end)
                elseif mode == 'bloxgloom:sand' then
                    pcall(function() c.schedule_entity(e.entity,0) end)
                end
                pcall(function() c.block(1600,80,0) end)
            else
                assert(c.schedule_entity(e.entity,nil))
            end
        end
    end
end"#;

fn tick(state: &mut State, id: crate::server::entities::EntityId, due: u64) {
    state
        .durability
        .queued
        .push_back(DurableRequest::EntityTick { id });
    process_durable_actions(state, TickId::new(due), Instant::now()).unwrap();
    complete_barrier(state, CommitBarrier::AllStaged).unwrap();
    assert!(state.durability.queued.is_empty());
    assert!(state.durability.pending.is_empty());
}

#[test]
fn luau_decisions_owned_entity_state_schedule_and_restart() {
    let fixture = Fixture::new();
    fixture.action(ENTITY_REGISTER, ENTITY_SOURCE);
    let mut state = fixture.open_decisions().unwrap();
    prepare_player(&mut state);
    let catalog = state.world.catalog_arc();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:stick").unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for mode in 1..=6 {
            let request = peer.request(mode);
            let result = peer.send(&request);
            assert!(
                !result.0,
                "caught invalid entity operation accepted: {mode}"
            );
        }
    });
    // Restart before a success: failed plans cannot be masked by later writes.
    let mut state = fixture.open_decisions().unwrap();
    prepare_player(&mut state);
    assert!(
        state
            .entities
            .query_mobile_aabb([6., 80., 0.], [8., 81., 1.])
            .unwrap()
            .is_empty()
    );
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        let result = peer.send(&request);
        assert!(result.0, "{}", result.1);
        assert!(peer.send(&request).0, "duplicate action receipt rejected");
    });
    let mut state = fixture.open_decisions().unwrap();
    let ids = state
        .entities
        .query_mobile_aabb([6., 80., 0.], [8., 81., 1.])
        .unwrap();
    assert_eq!(ids.len(), 2);
    let marker = *ids
        .iter()
        .find(|id| state.entities.public_view(**id).unwrap().payload == [1])
        .unwrap();
    let removed = *ids.iter().find(|id| **id != marker).unwrap();
    let due = state.entities.snapshot(marker).unwrap().next_tick.unwrap();
    tick(&mut state, marker, due - 1);
    assert_eq!(state.entities.public_view(marker).unwrap().payload, [1]);
    for (block, reason) in [
        (GLOWSTONE, "expected state byte"),
        (SAND, "integer out of bounds"),
        (AIR, "terrain unavailable"),
    ] {
        state.world.edit(5, 80, 0, block).unwrap();
        let error = crate::server::durable::actions::plan_durable_request(
            &mut state,
            &DurableRequest::EntityTick { id: marker },
            TickId::new(due),
        )
        .err()
        .expect("caught tick error published a plan");
        assert!(error.to_string().contains(reason), "{error}");
        assert_eq!(state.entities.public_view(marker).unwrap().payload, [1]);
        assert_eq!(
            state.entities.snapshot(marker).unwrap().next_tick,
            Some(due)
        );
        assert!(crate::server::drops::nearby(&state.entities, [8.5, 80.5, 0.5]).is_empty());
        if block == AIR {
            assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        }
    }
    // Satisfy the exact unavailable terrain and retry once on the same frozen
    // handler. No polling or backoff masquerades as input availability.
    state
        .world
        .get_chunk(crate::world::world_to_chunk(1600, 80, 0).0)
        .unwrap();
    tick(&mut state, marker, due);
    assert_eq!(state.entities.public_view(marker).unwrap().payload, [2]);
    assert_eq!(
        state.entities.snapshot(marker).unwrap().next_tick,
        Some(due + 2)
    );
    tick(&mut state, removed, due);
    assert!(state.entities.snapshot(removed).is_none());
    drop(state);
    let mut state = fixture.open_decisions().unwrap();
    assert_eq!(
        state.entities.snapshot(marker).unwrap().next_tick,
        Some(due + 2)
    );
    assert!(state.entities.snapshot(removed).is_none());
    tick(&mut state, marker, due + 2);
    assert_eq!(state.entities.public_view(marker).unwrap().payload, [3]);
    assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, None);
    drop(state);
    let state = fixture.open_decisions().unwrap();
    assert_eq!(state.entities.public_view(marker).unwrap().payload, [3]);
    assert_eq!(state.entities.snapshot(marker).unwrap().next_tick, None);
    assert_eq!(
        crate::server::drops::nearby(&state.entities, [8.5, 80.5, 0.5]).len(),
        1
    );
}

#[test]
fn luau_decisions_registration_ownership_bounds_and_source_identity() {
    let fixture = Fixture::new();
    for declaration in [
        "h.register_handler('foreign:removed',1,'BlockRemoved','bloxgloom:sand','demo:action')",
        "h.register_handler('demo:removed',0,'BlockRemoved','bloxgloom:sand','demo:action')",
        "h.register_handler('demo:removed',1,'BlockRemoved',nil,'demo:action')",
        "h.register_handler('demo:removed',1,'BlockRemoved','bloxgloom:stick','demo:action')",
        "h.register_handler('demo:removed',1,'PickupRequested','bloxgloom:stick','demo:action')",
        "h.register_handler('demo:removed',1,'EntityTick','foreign:marker','demo:action')",
        "h.register_handler('demo:removed',1,'EntityTick','demo:missing','demo:action')",
        "h.register_handler('demo:removed',1,'BlockRemoved','bloxgloom:sand','demo:missing')",
        "for i=1,2 do h.register_handler('demo:h'..i,1,'BlockRemoved','bloxgloom:sand','demo:action') end",
        "for i=1,33 do h.register_handler('demo:h'..i,1,'BlockRemoved','demo:t'..i,'demo:action') end",
    ] {
        fixture.action(
            &format!("return function(h) pcall(function() {declaration} end) end"),
            BLOCK_SOURCE,
        );
        assert!(fixture.open().is_err(), "{declaration}");
        assert!(!fixture.0.join("save").exists());
    }
    fixture.package("demo", "module action action.luau", BLOCK_REGISTER);
    assert!(fixture.open().is_err(), "undeclared capability accepted");
    assert!(!fixture.0.join("save").exists());
    fixture.action(BLOCK_REGISTER, BLOCK_SOURCE);
    drop(fixture.open().unwrap());
    let manifest = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    for (register, source) in [
        (
            BLOCK_REGISTER.to_owned(),
            format!("{BLOCK_SOURCE}\n-- changed decision"),
        ),
        (
            BLOCK_REGISTER.replace("removed',1", "removed',2"),
            BLOCK_SOURCE.to_owned(),
        ),
        (
            BLOCK_REGISTER.replace("'BlockRemoved'", "'BlockPlaced'"),
            BLOCK_SOURCE.to_owned(),
        ),
    ] {
        fixture.action(&register, &source);
        assert!(
            fixture.open().is_err(),
            "changed decision identity reopened save"
        );
        assert_eq!(
            std::fs::read(fixture.0.join("save/content.map")).unwrap(),
            manifest
        );
    }
    fixture.action(BLOCK_REGISTER, BLOCK_SOURCE);
    drop(fixture.open().unwrap());
}
