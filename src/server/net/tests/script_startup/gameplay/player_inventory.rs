//! Cross-profile finite inventories through the real listener and WAL recovery.
use super::*;
#[path = "player_inventory/concurrency.rs"]
mod concurrency;
#[path = "player_inventory/lifecycle.rs"]
mod lifecycle;
const TARGET: u128 = PROFILE + 1;
fn fixture(authority: bool) -> Fixture {
    let fixture = Fixture::new();
    fixture.package("demo", &format!("requires bloxgloom:actions/v1\n{}module action action.luau\nmodule ledger ledger.luau", if authority {"requires bloxgloom:players/v1\n"} else {""}),
        if authority {"return function(h) h.register_action('demo:shift',1,'Transfer','empty',nil,'demo:action'); h.register_player_lifecycle('demo:ledger',1,64,'','demo:ledger') end"}
        else {"return function(h) h.register_action('demo:shift',1,'Transfer','empty',nil,'demo:action') end"});
    std::fs::write(
        fixture.0.join("packages/demo/ledger.luau"),
        "return function() end",
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/action.luau"),format!(r#"return function(c,e)
        local target=c.profile_id('profile:{TARGET:032x}')
        local mode=string.byte(e.arguments,1)
        local before=c.inventory(target)
        assert(#before==36 and before[1].insert and before[1].extract)
        assert(not pcall(function() before[1].stack.count=1 end))
        if mode==9 then
            assert(#c.inventory('player')==36)
            c.message_player(c.player_by_profile(c.player_profile).session,'Inventory inspected')
            return
        end
        assert(not c.transfer_inventory('player',1,target,1,1),'different components merged')
        local count=if mode==10 or mode==12 then 1 else 3
        local destination=if mode==12 then 2 else 0
        assert(c.transfer_inventory('player',0,target,destination,count))
        assert(c.inventory(target)[destination+1].stack.components.bytes==string.char(0,255,3))
        c.set_profile_state('demo:ledger',target,'received','credited')
        c.set_profile_state('demo:ledger',c.player_profile,'sent','ack')
        if mode==1 then error('reject transfer') end
        if mode==2 then pcall(function() c.transfer_inventory('player',0,target,0,129) end) end
        if mode==3 then pcall(function() c.inventory(tostring(target)) end) end
        if mode==4 then pcall(function() c.inventory(c.player_by_profile(c.player_profile).session) end) end
    end"#)).unwrap();
    fixture
}
fn seeded(state: &State, source: u16, target: u16) {
    let item = state
        .world
        .catalog()
        .item_by_key("bloxgloom:stick")
        .unwrap();
    for (profile, count) in [(PROFILE, source), (TARGET, target)] {
        let mut inventory = Inventory::default();
        inventory.slots[0] = Some(Stack::with_components(item, count, 1, vec![0, 255, 3]).unwrap());
        inventory.slots[1] = Some(
            Stack::with_components(
                item,
                1,
                1,
                if profile == PROFILE { vec![8] } else { vec![9] },
            )
            .unwrap(),
        );
        state.inventory_store.save(profile, &inventory).unwrap();
    }
}
fn slot(peer: &mut Peer, index: usize, count: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while peer.inventory.slots[index].as_ref().map(|s| s.count) != Some(count) {
        peer.read(deadline);
    }
    assert_eq!(
        peer.inventory.slots[index]
            .as_ref()
            .unwrap()
            .components
            .as_ref()
            .unwrap()
            .bytes
            .as_ref(),
        [0, 255, 3]
    );
}
#[test]
fn profile_inventory_transfer_is_atomic_conserved_live_offline_and_restart_safe() {
    let fixture = fixture(true);
    let state = Box::new(fixture.open().unwrap());
    seeded(&state, 10, 124);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, Arc::clone(&catalog));
        for mode in 1..=4 {
            let request = actor.request(mode);
            let (accepted, reason) = actor.send(&request);
            assert!(!accepted, "mode {mode}: {reason}");
            assert_eq!(actor.inventory.slots[0].as_ref().unwrap().count, 10);
        }
        let request = actor.request(0);
        let (accepted, reason) = actor.send(&request);
        assert!(accepted, "{reason}");
        slot(&mut actor, 0, 7);
        assert!(actor.send(&request).0, "receipt replay rejected");
        let mut target = Peer::connect_profile(address, Arc::clone(&catalog), TARGET);
        slot(&mut target, 0, 127);
        let old_epoch = target.epoch;
        let request = actor.request(10);
        let (accepted, reason) = actor.send(&request);
        assert!(accepted, "{reason}");
        slot(&mut actor, 0, 6);
        slot(&mut target, 0, 128);
        assert!(actor.send(&request).0);
        let full = actor.request(0);
        assert!(!actor.send(&full).0, "overfilled 128 stack");
        let query = actor.request(9);
        assert!(
            actor.send(&query).0,
            "readonly inventory caused an identical WAL write"
        );
        drop(target);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::PlayerRoster { players, .. } = actor.read(deadline)
                && !players.iter().any(|p| p.profile == TARGET)
            {
                break;
            }
        }
        let offline = actor.request(12);
        let (accepted, reason) = actor.send(&offline);
        assert!(accepted, "{reason}");
        slot(&mut actor, 0, 5);
        let mut reconnect = Peer::connect_profile(address, catalog, TARGET);
        assert!(reconnect.epoch > old_epoch);
        slot(&mut reconnect, 0, 128);
        slot(&mut reconnect, 2, 1);
    });
    let state = Box::new(fixture.open().unwrap());
    let source = state.inventory_store.load(PROFILE).unwrap();
    let target = state.inventory_store.load(TARGET).unwrap();
    assert_eq!(source.slots[0].as_ref().unwrap().count, 5);
    assert_eq!(target.slots[0].as_ref().unwrap().count, 128);
    assert_eq!(target.slots[2].as_ref().unwrap().count, 1);
    assert_eq!(
        source.slots.iter().flatten().map(|s| s.count).sum::<u16>()
            + target.slots.iter().flatten().map(|s| s.count).sum::<u16>(),
        136
    );
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, Arc::clone(&catalog));
        let mut target = Peer::connect_profile(address, catalog, TARGET);
        slot(&mut actor, 0, 5);
        slot(&mut target, 0, 128);
        slot(&mut target, 2, 1);
        let query = actor.request(9);
        assert!(actor.send(&query).0);
    });
}
#[test]
fn cross_profile_inventory_requires_package_authority_even_for_reads() {
    let fixture = fixture(false);
    let state = Box::new(fixture.open().unwrap());
    seeded(&state, 10, 124);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, catalog);
        let request = actor.request(0);
        let (accepted, reason) = actor.send(&request);
        assert!(
            !accepted && reason.contains("profile inventory"),
            "{reason}"
        );
    });
    let state = fixture.open().unwrap();
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        10
    );
    assert_eq!(
        state.inventory_store.load(TARGET).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        124
    );
}

#[test]
fn corrupt_offline_inventory_rejects_caught_access_without_grant_or_server_failure() {
    let fixture = fixture(true);
    std::fs::write(
        fixture.0.join("packages/demo/action.luau"),
        format!(
            r#"return function(c)
        assert(c.give('player',{{item='bloxgloom:stick',count=1}}))
        pcall(function() c.inventory(c.profile_id('profile:{TARGET:032x}')) end)
    end"#
        ),
    )
    .unwrap();
    let state = Box::new(fixture.open().unwrap());
    seeded(&state, 10, 124);
    std::fs::write(
        fixture.0.join(format!("save/players/{TARGET:032x}.inv")),
        b"corrupt inventory",
    )
    .unwrap();
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, catalog);
        let request = actor.request(0);
        let (accepted, reason) = actor.send(&request);
        assert!(
            !accepted && reason.contains("offline inventory"),
            "{reason}"
        );
        assert_eq!(actor.inventory.slots[0].as_ref().unwrap().count, 10);
        protocol::write_client(&mut actor.stream, &ClientMessage::Ping { nonce: 219 }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(actor.read(deadline), ServerMessage::Pong { nonce: 219 }) {}
    });
}

#[test]
fn oversized_profile_inventory_transaction_is_denied_and_next_wal_action_succeeds() {
    use crate::server::{
        durable::{CommitAction, CommitBarrier, complete_barrier},
        parallel::OwnerKey,
        registry::SystemId,
        simulation::TickId,
    };
    use bloxgloom_host_api::players::State as ProfileState;
    let fixture = fixture(true);
    std::fs::write(fixture.0.join("packages/demo/main.luau"),"return function(h) h.register_action('demo:shift',1,'Large transaction','empty',nil,'demo:action'); h.register_player_lifecycle('demo:ledger',1,4096,string.rep('o',4096),'demo:ledger') end").unwrap();
    std::fs::write(fixture.0.join("packages/demo/action.luau"),r#"return function(c,e)
        if string.byte(e.arguments,1)==1 then
            c.message_player(c.player_by_profile(c.player_profile).session,'Still alive')
            return
        end
        for i=201,207 do
            local profile=c.profile_id('profile:0000000000000000000000000000'..string.format('%04x',i))
            assert(c.take(profile,0,1))
        end
        for i=201,264 do
            local profile=c.profile_id('profile:0000000000000000000000000000'..string.format('%04x',i))
            c.set_profile_state('demo:ledger',profile,string.rep('n',4096),string.rep('u',1024))
        end
    end"#).unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    let reg = state
        .world
        .catalog()
        .player_lifecycles()
        .next()
        .unwrap()
        .clone();
    let system = SystemId::new(&reg.key).unwrap();
    let initial = ProfileState {
        data: vec![b'o'; 4096],
        public_data: vec![b'p'; 1024],
    };
    let mut owner_changes = Vec::new();
    for profile in 201..=264 {
        owner_changes.extend(
            crate::server::players::state::prepare(
                &state.system_runtime,
                &reg,
                profile,
                initial.clone(),
                None,
            )
            .unwrap(),
        );
    }
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: vec![],
        terrain_reads: Default::default(),
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entities: None,
        entity_wakes: vec![],
        player_publication: None,
        owner_changes,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    let item = state
        .world
        .catalog()
        .item_by_key("bloxgloom:stick")
        .unwrap();
    for profile in 201..=207 {
        let mut inventory = Inventory::default();
        for slot in &mut inventory.slots {
            *slot = Some(Stack::with_components(item, 2, 1, vec![42; 1024]).unwrap());
        }
        state.inventory_store.save(profile, &inventory).unwrap();
    }
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, catalog);
        let large = actor.request(0);
        let (accepted, reason) = actor.send(&large);
        assert!(!accepted && reason.contains("action exceeds"), "{reason}");
        let small = actor.request(1);
        let (accepted, reason) = actor.send(&small);
        assert!(
            accepted,
            "writer did not survive oversized candidate: {reason}"
        );
    });
    let state = fixture.open().unwrap();
    for profile in 201..=264 {
        let (_, value) = state
            .system_runtime
            .owner_snapshot(&system, OwnerKey::Profile(profile))
            .unwrap();
        assert_eq!(value.get::<ProfileState>().unwrap(), &initial);
    }
    for profile in 201..=207 {
        assert!(
            state
                .inventory_store
                .load(profile)
                .unwrap()
                .slots
                .iter()
                .flatten()
                .all(|s| s.count == 2)
        );
    }
}
