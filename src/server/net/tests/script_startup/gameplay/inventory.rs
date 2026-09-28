use super::*;

#[test]
fn luau_take_and_spawn_stack_preserve_binary_components_across_receipt_and_restart() {
    let fixture = Fixture::new();
    fixture.action(
        &REGISTER.replace("'item', 'bloxgloom:stick'", "'empty', nil"),
        "return function(c,e) local s=c.take('player',0,1); assert(s); c.spawn_stack(e.position[1],e.position[2]+0.8,e.position[3],s,60000) end",
    );
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let original = Stack::with_components(
        catalog.item_by_key("bloxgloom:stick").unwrap(),
        2,
        1,
        vec![0, 255, 3],
    )
    .unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(original.clone());
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        assert!(peer.send(&request).0, "same receipt cannot throw twice");
        peer.inventory_at(1);
    });
    let state = fixture.open().unwrap();
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    let drops: Vec<_> = state
        .entities
        .record_values()
        .filter(|record| record.entity_type == crate::server::drops::DROP_ENTITY_TYPE)
        .map(|record| crate::server::drops::stack(&state.entities, record.id).unwrap())
        .collect();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].count, 1);
    assert_eq!(drops[0].components, original.components);
}

const SOURCE: &str = r#"return function(c,e)
    local slots = c.inventory('player')
    assert(#slots == 36 and slots[1].insert and slots[1].extract)
    assert(not pcall(function() slots[1].stack.components.bytes = 'changed' end))
    assert(not pcall(function() slots[1].stack.count = 128 end))
    local taken = c.take('player',0,2)
    assert(taken.count == 2 and taken.components.version == 1)
    assert(taken.components.bytes == string.char(0,255,3))
    assert(c.give('player',taken))
    assert(c.transfer_inventory('player',0,'player',1,1))
    assert(c.inventory('player')[1].stack.count == 5)
    assert(c.take('player',2,1) == nil)
    local mode = string.byte(e.arguments,1)
    if mode == 1 then error('rollback inventory') end
    if mode == 2 then pcall(function() c.give('player',{item='bloxgloom:stick',count=129}) end) end
    if mode == 3 then pcall(function() c.give('player',{item='bloxgloom:stick',count=1,components={version=1,bytes=string.rep('x',1025)}}) end) end
    if mode == 4 then pcall(function() c.give('player',{item='bloxgloom:stick',count=1,components={version=0,bytes='x'}}) end) end
    if mode == 5 then pcall(function() c.inventory('other:profile') end) end
    if mode == 6 then pcall(function() c.inventory({entity_lo=0,entity_hi=0}) end) end
    if mode == 7 then pcall(function() c.inventory(setmetatable({}, {__index=function() error('metamethod invoked') end})) end) end
    if mode == 8 then pcall(function() c.take('player',256,1) end) end
    if mode == 9 then pcall(function() c.give('player',{item='demo:unknown',count=1}) end) end
end"#;

fn prepare(state: &mut State) {
    state.spawn_anchor = [0.5, 80., 0.5];
    for (y, block) in [(79, crate::world::STONE), (80, AIR), (81, AIR)] {
        state.world.edit(0, y, 0, block).unwrap();
    }
}

#[test]
fn luau_inventory_exact_binary_reads_give_take_and_error_rollback_restart() {
    let fixture = Fixture::new();
    fixture.action(REGISTER, SOURCE);
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let mut original = Inventory::default();
    original.slots[0] = Some(
        Stack::with_components(
            catalog.item_by_key("bloxgloom:stick").unwrap(),
            6,
            1,
            vec![0, 255, 3],
        )
        .unwrap(),
    );
    state.inventory_store.save(PROFILE, &original).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for mode in 1..=9 {
            let request = peer.request(mode);
            let (accepted, reason) = peer.send(&request);
            assert!(!accepted, "mode {mode}: {reason}");
            assert!(
                !reason.contains("metamethod invoked"),
                "raw field decoding invoked script"
            );
        }
    });
    let state = Box::new(fixture.open().unwrap());
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots,
        original.slots
    );
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        let result = peer.send(&request);
        assert!(result.0, "{}", result.1);
        peer.inventory_at(5);
        assert_eq!(
            peer.inventory.slots[1].as_ref().unwrap().components,
            original.slots[0].as_ref().unwrap().components
        );
    });
    let state = fixture.open().unwrap();
    let saved = state.inventory_store.load(PROFILE).unwrap();
    assert_eq!(saved.slots[0].as_ref().unwrap().count, 5);
    assert_eq!(saved.slots[1].as_ref().unwrap().count, 1);
    assert_eq!(
        saved.slots.iter().flatten().map(|s| s.count).sum::<u16>(),
        6
    );
}

const PICKUP_REGISTER: &str = "return function(h) h.register_handler('demo:pickup',1,'PickupRequested','bloxgloom:drop','demo:action') end";
const PICKUP: &str = r#"return function(c,e)
    assert(e.kind == 'PickupRequested' and #e.drops <= 32)
    assert(e.position[1] == 0.5 and e.position[2] == 80)
    assert(not pcall(function() e.drops[1].count = 128 end))
    assert(not pcall(function() e.drops[1] = nil end))
    for _,drop in e.drops do
        local slots = c.inventory(drop)
        assert(#slots == 1 and not slots[1].insert and slots[1].extract)
        local stack = slots[1].stack
        assert(stack.components.version == 1 and stack.components.bytes == string.char(0,255,3))
        assert(stack.count == 2 and drop.count == 2)
        assert(not c.give(drop,stack))
        assert(not c.transfer_inventory(drop,0,'player',0,1)) -- different components
        assert(c.inventory(drop)[1].stack.count == 2)
        assert(c.transfer_inventory(drop,0,'player',2,1))
        assert(c.collect_drop(drop,1) == 1)
        assert(c.inventory(drop)[1].stack == nil)
    end
end"#;

#[test]
fn luau_automatic_pickup_exact_components_conservation_and_restart() {
    let fixture = Fixture::new();
    fixture.action(PICKUP_REGISTER, PICKUP);
    let mut state = Box::new(fixture.open().unwrap());
    prepare(&mut state);
    let catalog = state.world.catalog_arc();
    let item = catalog.item_by_key("bloxgloom:stick").unwrap();
    let mut original = Inventory::default();
    // Destination 0 has capacity but a distinct component identity: pickup must
    // not collapse it into the binary stack, even though the item IDs match.
    original.slots[0] = Some(Stack::with_components(item, 1, 1, vec![9]).unwrap());
    original.slots[1] = Some(Stack::with_components(item, 4, 1, vec![0, 255, 3]).unwrap());
    state.inventory_store.save(PROFILE, &original).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
        protocol::write_client(
            &mut peer.stream,
            &ClientMessage::DropStack {
                action_id,
                slot: 1,
                count: 2,
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut accepted, mut picked) = (false, 0);
        // Real automatic admission, including the host-owned throw delay. Wait
        // for explicit receipt, pickup notification AND final inventory, not a sleep.
        while !accepted || picked != 2 || peer.inventory.revision < 2 {
            match peer.read(deadline) {
                ServerMessage::ActionResult {
                    action_id: id,
                    accepted: ok,
                    reason,
                } if id == action_id => {
                    assert!(ok, "{reason}");
                    accepted = true;
                }
                ServerMessage::Pickups { items } => {
                    picked += items.iter().map(|d| d.count).sum::<u16>()
                }
                _ => {}
            }
        }
        assert_eq!(peer.inventory.slots[0], original.slots[0]);
        assert_eq!(peer.inventory.slots[1].as_ref().unwrap().count, 3);
        assert_eq!(peer.inventory.slots[2].as_ref().unwrap().count, 1);
        assert_eq!(
            peer.inventory.slots[2].as_ref().unwrap().components,
            original.slots[1].as_ref().unwrap().components
        );
    });
    let state = fixture.open().unwrap();
    let saved = state.inventory_store.load(PROFILE).unwrap();
    assert_eq!(
        saved.slots.iter().flatten().map(|s| s.count).sum::<u16>(),
        5
    );
    assert_eq!(saved.slots[1].as_ref().unwrap().count, 3);
    assert_eq!(saved.slots[2].as_ref().unwrap().count, 1);
    assert_eq!(
        saved.slots[2].as_ref().unwrap().components,
        original.slots[1].as_ref().unwrap().components
    );
    assert!(crate::server::drops::nearby(&state.entities, [0.5, 80., 0.5]).is_empty());
}

// Planner-only fixture: a socket supplies the existing Client shape, not an
// alternate pickup policy or listener test. Automatic dispatch is tested above.
fn actor(state: &mut State, inventory: Inventory) -> TcpStream {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (sender, _receiver) = state.outbound.client_queue();
    state.clients.insert(
        1,
        crate::server::Client {
            profile: PROFILE,
            inventory,
            last_drops_revision: u64::MAX,
            last_drop_anchor: [i32::MAX; 3],
            last_sent_drops: Vec::new(),
            sender,
            socket,
            sent: Default::default(),
            sent_epochs: Default::default(),
            sent_block_versions: Default::default(),
            sent_entity_revisions: Default::default(),
            next_snapshot_epoch: 1,
            center: crate::world::world_to_chunk(0, 80, 0).0,
            radius: crate::server::DEFAULT_VIEW,
            movement: crate::server::movement::MovementState::new([0.5, 80., 0.5], 0),
            pending_moves: Default::default(),
        },
    );
    peer
}

#[test]
fn luau_pickup_host_credit_eligibility_permissions_and_caught_errors_rollback() {
    use crate::server::{
        durable::{DurableRequest, actions::plan_durable_request},
        simulation::TickId,
    };
    // Errors after valid staged mutations must never admit a partial candidate.
    // Cases also prove script access cannot replace the host candidate authority.
    for (body, expected) in [
        ("local s=c.take(e.drops[1],0,1)", "did not credit"),
        (
            "local s=c.take(e.drops[1],0,1); assert(c.give('player',{item=s.item,count=1}))",
            "without crediting",
        ),
        (
            "assert(c.transfer_inventory({entity_lo=2,entity_hi=0},0,'player',0,1))",
            "ineligible drop",
        ),
        (
            "assert(c.transfer_inventory(e.drops[1],0,'player',0,1)); error('abort pickup')",
            "abort pickup",
        ),
        (
            "assert(c.transfer_inventory(e.drops[1],0,'player',0,1)); pcall(function() c.take('player',-1,1) end)",
            "integer",
        ),
        (
            "assert(c.transfer_inventory(e.drops[1],0,'player',0,1)); pcall(function() c.inventory({entity_lo=3,entity_hi=0}) end)",
            "outside interaction radius",
        ),
        (
            "assert(c.transfer_inventory(e.drops[1],0,'player',0,1)); pcall(function() c.entity_state(e.drops[1].entity_lo,e.drops[1].entity_hi) end)",
            "no general gameplay state",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.action(PICKUP_REGISTER, &format!("return function(c,e) {body} end"));
        let mut state = fixture.open().unwrap();
        let original = Inventory::default();
        let _peer = actor(&mut state, original.clone());
        let item = state
            .world
            .catalog()
            .item_by_key("bloxgloom:stick")
            .unwrap();
        for (index, x) in [0.5, 4.5, 20.5].into_iter().enumerate() {
            let spawn = crate::server::drops::plan_spawn_stack(
                &state.entities,
                state.world.catalog(),
                [x, 80.5, 0.5],
                Stack::with_components(item, 2, 1, vec![0, 255, 3]).unwrap(),
                Duration::ZERO,
                1,
                crate::server::drops::unix_ms(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(spawn.entity_id().get(), index as u64 + 1);
            state.entities.apply_committed(spawn).unwrap();
        }
        let error = plan_durable_request(
            &mut state,
            &DurableRequest::Pickup { id: 1 },
            TickId::new(1),
        )
        .err()
        .expect("invalid pickup admitted");
        assert!(error.to_string().contains(expected), "{body}: {error}");
        assert_eq!(state.clients[&1].inventory.slots, original.slots);
        for id in 1..=3 {
            assert_eq!(
                crate::server::drops::stack(
                    &state.entities,
                    crate::server::entities::EntityId::new(id).unwrap()
                )
                .unwrap()
                .count,
                2
            );
        }
        assert!(state.durability.pending.is_empty());
    }
}

#[test]
fn luau_inventory_capacity_failures_and_delayed_drop_leave_every_slot_unchanged() {
    use crate::server::gameplay::{OperationInput, Participants, plan_removals};
    use bloxgloom_host_api::gameplay::Event;
    let fixture = Fixture::new();
    fixture.action(
        REGISTER,
        r#"return function(c,e)
        assert(not c.give('player',{item='bloxgloom:stick',count=2}))
        assert(not c.transfer_inventory('player',0,'player',1,2))
        local drop = {entity_lo=1,entity_hi=0}
        assert(not c.inventory(drop)[1].extract)
        assert(c.take(drop,0,1) == nil)
        assert(not c.transfer_inventory(drop,0,'player',1,1))
        assert(not c.give(drop,{item='bloxgloom:stick',count=1}))
        assert(c.inventory('player')[1].stack.count == 128)
        assert(c.inventory('player')[2].stack.count == 127)
    end"#,
    );
    let mut state = fixture.open().unwrap();
    let item = state
        .world
        .catalog()
        .item_by_key("bloxgloom:stick")
        .unwrap();
    let mut inventory = Inventory::default();
    inventory.slots.fill(Some(Stack::new(item, 128)));
    inventory.slots[1] = Some(Stack::new(item, 127));
    let spawn = crate::server::drops::plan_spawn_stack(
        &state.entities,
        state.world.catalog(),
        [0.5, 80.5, 0.5],
        Stack::new(item, 2),
        Duration::from_millis(u32::MAX.into()),
        1,
        crate::server::drops::unix_ms(),
    )
    .unwrap()
    .unwrap();
    state.entities.apply_committed(spawn).unwrap();
    let planned = plan_removals(
        &mut state.world,
        &mut Default::default(),
        &mut Vec::new(),
        OperationInput {
            edits: &[],
            removals: &[],
            seed: 7,
            tick: 1,
            action: Some(Event::ActionRequested {
                action: "demo:shift".into(),
                position: [0.5, 80., 0.5],
                cell: None,
                entity: None,
                slot: 0,
                arguments: vec![],
            }),
        },
        Participants {
            actor: Some((PROFILE, &inventory)),
            actor_position: Some([0.5, 80.0, 0.5]),
            admin: false,
            entities: &state.entities,
        },
    )
    .unwrap();
    // An inventory read remains a transaction participant even with no write.
    let captured = planned.inventory.unwrap();
    assert_eq!(captured.slots, inventory.slots);
    assert_eq!(captured.revision, inventory.revision);
    assert!(planned.drop_takes.is_empty());
    assert!(planned.entity_updates.is_empty());
}
