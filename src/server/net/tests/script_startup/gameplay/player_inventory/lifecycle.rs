//! Lifecycle decisions can atomically use actor and offline profile inventories.
use super::*;

#[test]
fn offline_profile_tick_loads_asynchronously_and_commits_both_inventories() {
    use crate::server::{
        durable::{CommitAction, CommitBarrier, complete_barrier},
        simulation::TickId,
    };
    let fixture = fixture(true);
    std::fs::write(
        fixture.0.join("packages/demo/ledger.luau"),
        format!(
            r#"return function(c,e)
        assert(e.kind=='ProfileTick' and e.player==nil)
        assert(tostring(c.player_profile)=='profile:{TARGET:032x}')
        local other=c.profile_id('profile:{PROFILE:032x}')
        assert(c.transfer_inventory('player',0,other,2,1))
        return {{state='offline transfer'}}
    end"#
        ),
    )
    .unwrap();
    let mut state = fixture.open().unwrap();
    seeded(&state, 10, 124);
    let reg = state
        .world
        .catalog()
        .player_lifecycles()
        .next()
        .unwrap()
        .clone();
    let changes = crate::server::players::state::prepare(
        &state.system_runtime,
        &reg,
        TARGET,
        bloxgloom_host_api::players::State {
            data: b"scheduled".to_vec(),
            public_data: vec![],
        },
        Some(1),
    )
    .unwrap();
    let bootstrap = CommitAction {
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
        owner_changes: changes,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &bootstrap, None)
            .unwrap()
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    crate::server::players::drive(&mut state, TickId::new(1)).unwrap();
    assert!(
        !state.durability.inventory_overlay.contains_key(&TARGET),
        "first load should defer"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !state.durability.inventory_overlay.contains_key(&TARGET) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
        crate::server::players::drive(&mut state, TickId::new(1)).unwrap();
        complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    }
    assert_eq!(
        state.durability.inventory_overlay[&TARGET].slots[0]
            .as_ref()
            .unwrap()
            .count,
        123
    );
    assert_eq!(
        state.durability.inventory_overlay[&PROFILE].slots[2]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    drop(state);
    let state = fixture.open().unwrap();
    assert_eq!(
        state.inventory_store.load(TARGET).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        123
    );
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[2]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}

#[test]
fn joined_cross_profile_inventory_and_state_commit_together_once() {
    let fixture = fixture(true);
    std::fs::write(fixture.0.join("packages/demo/ledger.luau"), format!(r#"return function(c,e)
        if e.kind=='PlayerJoined' and tostring(e.profile)=='profile:{PROFILE:032x}' and e.state=='' then
            local target=c.profile_id('profile:{TARGET:032x}')
            assert(c.transfer_inventory('player',0,target,2,1))
            c.set_profile_state('demo:ledger',target,'received','credited')
            return {{state='joined'}}
        end
    end"#)).unwrap();
    let state = Box::new(fixture.open().unwrap());
    seeded(&state, 10, 124);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, Arc::clone(&catalog));
        slot(&mut actor, 0, 9);
        let mut target = Peer::connect_profile(address, catalog, TARGET);
        slot(&mut target, 2, 1);
    });
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut actor = Peer::connect(address, Arc::clone(&catalog));
        let mut target = Peer::connect_profile(address, catalog, TARGET);
        slot(&mut actor, 0, 9);
        slot(&mut target, 2, 1);
    });
    let state = fixture.open().unwrap();
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        9
    );
    let system = crate::server::registry::SystemId::new("demo:ledger").unwrap();
    let cell = state
        .system_runtime
        .owner_snapshot(&system, crate::server::parallel::OwnerKey::Profile(TARGET))
        .unwrap();
    assert_eq!(
        cell.1
            .get::<bloxgloom_host_api::players::State>()
            .unwrap()
            .data,
        b"received"
    );
}
