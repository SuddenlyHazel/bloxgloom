//! Entirely local-package schemas: no native registration fixture or alternate codec.
use super::*;

const REGISTER_ENTITY: &str = "h.register_entity('demo:marker',1,3,1,2)";
const REGISTER: &str = r#"return function(h)
    h.register_entity('demo:marker',1,3,1,2)
    h.register_action('demo:shift',1,'Spawn','item','bloxgloom:stick','demo:action')
    h.register_handler('demo:tick',1,'EntityTick','demo:marker','demo:action')
end"#;
const SOURCE: &str = r#"return function(c,e)
    if e.kind == 'ActionRequested' then
        c.spawn_entity('demo:marker',6.5,80.5,0.5,string.char(1,0,255))
        local mode = string.byte(e.arguments,1)
        if mode == 1 then pcall(function() c.spawn_entity('demo:marker',7,80,0,'xx') end) end
        if mode == 2 then pcall(function() c.spawn_entity('demo:marker',7,80,0,'xxxx') end) end
        if mode == 3 then error('rollback after valid spawn') end
    else
        assert(e.kind == 'EntityTick')
        local bytes = c.entity_state(e.entity)
        if bytes == string.char(1,0,255) then
            assert(c.update_entity(e.entity,string.char(2,0,255)))
            c.schedule_entity(e.entity,100000)
            c.spawn_drop(8.5,80.5,0.5,'bloxgloom:seeds',1,4294967295)
        else
            assert(bytes == string.char(2,0,255))
            assert(c.update_entity(e.entity,string.char(3,0,255)))
            c.schedule_entity(e.entity,nil)
            if c.block(5,80,0).state == 'bloxgloom:glowstone' then
                pcall(function() c.update_entity(e.entity,'xx') end)
            end
        end
    end
end"#;

fn prepare(state: &mut State) {
    state.spawn_anchor = [0.5, 80., 0.5];
    for (y, block) in [(79, crate::world::STONE), (80, AIR), (81, AIR)] {
        state.world.edit(0, y, 0, block).unwrap();
    }
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        state
            .world
            .catalog()
            .item_by_key("bloxgloom:stick")
            .unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
}

impl Fixture {
    fn open_entities(&self) -> io::Result<Box<State>> {
        self.open().map(Box::new)
    }
}

#[test]
fn luau_entity_schema_loopback_spawn_due_callback_and_recovery() {
    let fixture = Fixture::new();
    fixture.action(REGISTER, SOURCE);
    let mut state = fixture.open_entities().unwrap();
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for mode in 1..=3 {
            let request = peer.request(mode);
            assert!(!peer.send(&request).0, "invalid spawn accepted: {mode}");
        }
    });
    let mut state = fixture.open_entities().unwrap();
    assert!(
        state
            .entities
            .query_mobile_aabb([6., 80., 0.], [9., 81., 1.])
            .unwrap()
            .is_empty()
    );
    assert!(crate::server::drops::nearby(&state.entities, [8.5, 80.5, 0.5]).is_empty());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let seeds = catalog.item_by_key("bloxgloom:seeds").unwrap();
    let marker = catalog.entity_type_id_by_key("demo:marker").unwrap();
    // Running definitions/handlers are frozen; no filesystem read on spawn or tick.
    std::fs::write(
        fixture.0.join("packages/demo/action.luau"),
        "error('changed file')",
    )
    .unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        protocol::write_client(&mut peer.stream, &request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut accepted, mut ticked, mut projected) = (false, false, false);
        // Watch both outcomes together, without relying on completion/receipt
        // ordering. A real replicated drop is the due callback's commit signal.
        while !accepted || !ticked || !projected {
            match peer.read(deadline) {
                ServerMessage::ActionResult {
                    accepted: result,
                    reason,
                    ..
                } => {
                    assert!(result, "{reason}");
                    accepted = true;
                }
                ServerMessage::Drops { items, .. } => {
                    ticked |= items
                        .iter()
                        .any(|drop| drop.item == seeds && drop.count == 1);
                }
                ServerMessage::WorldCommitPart(part) => {
                    for change in part.entities {
                        if let protocol::PublicEntityChange::Upsert(entity) = change
                            && entity.entity_type == marker
                        {
                            assert_eq!(entity.payload.len(), 1, "private suffix leaked");
                            projected |= entity.payload == [2];
                        }
                    }
                }
                ServerMessage::EntitySnapshotPage(page) => {
                    for entity in page
                        .entities
                        .iter()
                        .filter(|entity| entity.entity_type == marker)
                    {
                        assert_eq!(entity.payload.len(), 1, "private suffix leaked");
                        projected |= entity.payload == [2];
                    }
                }
                _ => {}
            }
        }
        assert!(peer.send(&request).0, "duplicate spawn receipt rejected");
    });
    fixture.action(REGISTER, SOURCE);
    let state = fixture.open_entities().unwrap();
    let ids = state
        .entities
        .query_mobile_aabb([6., 80., 0.], [7., 81., 1.])
        .unwrap()
        .into_iter()
        .filter(|id| {
            state.entities.snapshot(*id).unwrap().entity_type
                == state
                    .world
                    .catalog()
                    .entity_type_id_by_key("demo:marker")
                    .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 1, "duplicate request spawned twice");
    let id = ids[0];
    let snapshot = state.entities.snapshot(id).unwrap();
    assert_eq!(
        snapshot.private_payload.downcast_ref::<Vec<u8>>().unwrap(),
        &[2, 0, 255]
    );
    assert_eq!(state.entities.public_view(id).unwrap().payload, [2]);
    let due = snapshot.next_tick.unwrap();
    assert!(due >= 100000);
    assert_eq!(
        crate::server::drops::nearby(&state.entities, [8.5, 80.5, 0.5]).len(),
        1
    );
    drop(state);
    let mut state = fixture.open_entities().unwrap();
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, Some(due));
    assert_eq!(state.entities.public_view(id).unwrap().payload, [2]);
    use crate::server::{
        durable::{self, CommitBarrier, DurableRequest},
        simulation::TickId,
    };
    state.world.edit(5, 80, 0, GLOWSTONE).unwrap();
    let error = durable::actions::plan_durable_request(
        &mut state,
        &DurableRequest::EntityTick { id },
        TickId::new(due),
    )
    .err()
    .expect("caught invalid update published a plan");
    assert!(error.to_string().contains("fixed byte length"), "{error}");
    assert_eq!(state.entities.public_view(id).unwrap().payload, [2]);
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, Some(due));
    state.world.edit(5, 80, 0, AIR).unwrap();
    // Recovery rebuilds the actual scheduler index. Admit from it, not a
    // test-injected EntityTick request, and advance one logical due boundary.
    durable::queue_interaction_actions(&mut state, TickId::new(due));
    assert!(
        state
            .durability
            .queued
            .iter()
            .any(|r| matches!(r, DurableRequest::EntityTick { id: queued } if *queued == id))
    );
    durable::process_durable_actions(&mut state, TickId::new(due), Instant::now()).unwrap();
    durable::complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    assert_eq!(state.entities.public_view(id).unwrap().payload, [3]);
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, None);
    drop(state);
    let state = fixture.open_entities().unwrap();
    assert_eq!(state.entities.public_view(id).unwrap().payload, [3]);
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, None);
}

#[test]
fn luau_entity_schema_requires_a_targeted_tick_handler_for_scheduling() {
    use crate::server::{
        durable::{self, CommitBarrier, DurableRequest},
        simulation::TickId,
    };
    let fixture = Fixture::new();
    let register = r#"return function(h)
        h.register_entity('demo:marker',1,3,1,nil)
        h.register_entity('demo:other',1,3,1,nil)
        h.register_action('demo:shift',1,'Spawn','item','bloxgloom:stick','demo:action')
        h.register_handler('demo:tick',1,'EntityTick','demo:other','demo:action')
    end"#;
    let source = r#"return function(c,e)
        assert(e.kind == 'ActionRequested', 'passive entity was ticked')
        if #e.arguments == 1 then
            c.spawn_entity('demo:marker',6.5,80.5,0.5,string.char(1,0,255))
        else
            local function half(offset)
                local a,b,d,f = string.byte(e.arguments,offset,offset+3)
                return a + b*256 + d*65536 + f*16777216
            end
            local lo,hi = half(1),0
            assert(c.update_entity(lo,hi,string.char(2,0,255)))
            pcall(function() c.schedule_entity(lo,hi,2) end)
        end
    end"#;
    // An unrelated EntityTick owner must not authorize marker's initial due.
    fixture.action(
        &register.replace("marker',1,3,1,nil", "marker',1,3,1,2"),
        source,
    );
    let error = fixture
        .open_entities()
        .err()
        .expect("handlerless due accepted");
    assert!(
        error
            .to_string()
            .contains("demo:marker: scheduled entity requires a tick handler"),
        "{error}"
    );
    assert!(!fixture.0.join("save").exists());

    fixture.action(register, source);
    let mut state = fixture.open_entities().unwrap();
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let marker = catalog.entity_type_id_by_key("demo:marker").unwrap();
    let other = catalog.entity_type_id_by_key("demo:other").unwrap();
    assert!(
        !state
            .entities
            .types()
            .tickable_types()
            .any(|ty| ty == marker)
    );
    assert!(
        state
            .entities
            .types()
            .tickable_types()
            .any(|ty| ty == other)
    );
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
    });

    let mut state = fixture.open_entities().unwrap();
    let ids = state
        .entities
        .query_mobile_aabb([6., 80., 0.], [7., 81., 1.])
        .unwrap();
    assert_eq!(ids.len(), 1);
    let id = ids[0];
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, None);
    assert!(
        !state
            .entities
            .types()
            .tickable_types()
            .any(|ty| ty == marker)
    );
    assert!(state.entities.suspended_tick_after(None).is_none());
    assert!(
        state
            .entities
            .due_tick_entries(u64::MAX, None, 8)
            .is_empty()
    );
    durable::queue_interaction_actions(&mut state, TickId::new(100));
    assert!(!state.durability.queued.iter().any(|request| matches!(
        request,
        DurableRequest::EntityTick { .. } | DurableRequest::EntityWake { .. }
    )));
    durable::process_durable_actions(&mut state, TickId::new(100), Instant::now()).unwrap();
    durable::complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();

    // The existing runtime guard must also reject activation and roll back a
    // prior staged state write, even when Luau catches the schedule error.
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let mut request = peer.request(0);
        let ClientMessage::EntityInteract { payload, .. } = &mut request else {
            unreachable!()
        };
        let mut decoded = Request::decode(payload).unwrap();
        decoded.arguments = u32::try_from(id.get()).unwrap().to_le_bytes().to_vec();
        *payload = decoded.encode().unwrap();
        let (accepted, reason) = peer.send(&request);
        assert!(!accepted, "handlerless scheduling accepted");
        // Wire rejection reasons are truncated to the protocol's short bound.
        assert!(
            reason.starts_with("demo:shift: demo:marker has no "),
            "{reason}"
        );
    });
    let state = fixture.open_entities().unwrap();
    assert_eq!(state.entities.snapshot(id).unwrap().next_tick, None);
    assert_eq!(state.entities.public_view(id).unwrap().payload, [1]);
    assert!(state.entities.suspended_tick_after(None).is_none());
}

#[test]
fn luau_entity_schema_declaration_validation_and_identity() {
    let fixture = Fixture::new();
    for declaration in [
        "h.register_entity('foreign:marker',1,3,1,nil)",
        "h.register_entity('demo:bad:key',1,3,1,nil)",
        "h.register_entity('demo:marker',0,3,1,nil)",
        "h.register_entity('demo:marker',65536,3,1,nil)",
        "h.register_entity('demo:marker',1,0,0,nil)",
        "h.register_entity('demo:marker',1,65536,0,nil)",
        "h.register_entity('demo:marker',1,3,4,nil)",
        "h.register_entity('demo:marker',1,5000,4097,nil)",
        "h.register_entity('demo:marker',1,3,-1,nil)",
        "h.register_entity('demo:marker',1,3,1,0)",
        "h.register_entity('demo:marker',1,3,1,100001)",
        "h.register_entity('demo:marker',1,3,1,1.5)",
        "h.register_entity('demo:marker',1,3,1,2)", // No EntityTick decision owner.
        "h.register_entity('demo:marker',1,3,1,nil); h.register_entity('demo:marker',1,3,1,nil)",
        "for i=1,33 do h.register_entity('demo:e'..i,1,1,0,nil) end",
        "h.register_entity({},1,3,1,nil)",
    ] {
        fixture.action(
            &format!("return function(h) pcall(function() {declaration} end) end"),
            SOURCE,
        );
        assert!(fixture.open().is_err(), "{declaration}");
        assert!(!fixture.0.join("save").exists());
    }
    let suspended = format!(
        "return function(h) {} end",
        REGISTER_ENTITY.replace(",2)", ",nil)")
    );
    fixture.package("demo", "", &suspended);
    assert!(fixture.open().is_err(), "undeclared capability accepted");
    assert!(!fixture.0.join("save").exists());

    // Suspended entities retain their explicit schema across behavior-only edits.
    fixture.action(&suspended, SOURCE);
    fixture.reopen();
    let manifest = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    fixture.action(&suspended, &format!("{SOURCE}\n-- changed source"));
    fixture.reopen();
    for (register, source) in [
        (
            suspended.replace("marker',1", "marker',2"),
            SOURCE.to_owned(),
        ),
        (suspended.replace(",3,1,", ",4,1,"), SOURCE.to_owned()),
        (suspended.replace(",3,1,", ",3,0,"), SOURCE.to_owned()),
    ] {
        fixture.action(&register, &source);
        assert!(fixture.open().is_err(), "changed schema reopened save");
        assert_eq!(
            std::fs::read(fixture.0.join("save/content.map")).unwrap(),
            manifest
        );
    }
    fixture.action(&suspended, SOURCE);
    fixture.reopen();
}

#[test]
fn luau_entity_schema_fixed_binary_and_projection_bounds() {
    let fixture = Fixture::new();
    fixture.action(
        r#"return function(h)
        h.register_entity('demo:empty',1,1,0,nil)
        h.register_entity('demo:large',65535,65535,4096,nil)
        for i=1,30 do h.register_entity('demo:e'..i,1,1,0,nil) end
    end"#,
        SOURCE,
    );
    let state = fixture.open().unwrap();
    let catalog = state.world.catalog();
    let empty = catalog.gameplay_entity("demo:empty").unwrap();
    assert!(empty.state.public(&[255]).unwrap().is_empty());
    assert!(empty.state.validate(&[]).is_err());
    let large = catalog.gameplay_entity("demo:large").unwrap();
    let bytes: Vec<_> = (0..65535).map(|n| n as u8).collect();
    assert_eq!(large.state.public(&bytes).unwrap(), bytes[..4096]);
    assert!(large.state.public(&bytes[..65534]).is_err());
}
