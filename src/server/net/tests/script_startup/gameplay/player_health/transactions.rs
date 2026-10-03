use super::*;

#[test]
fn player_health_failed_wal_receipt_publishes_neither_death_nor_hook_reward() {
    use crate::server::{
        durable::{CommitAction, CommitBarrier, TerrainReads, complete_barrier},
        gameplay::{OperationInput, Participants},
    };
    use bloxgloom_host_api::gameplay::{Event, Player};
    let fixture = fixture();
    let mut state = fixture.open().unwrap();
    let catalog = state.world.catalog_arc();
    let before = Inventory::default();
    let players = [Player {
        profile: PROFILE,
        session: 1,
        entity: 0,
        name: "health-fixture".into(),
        position: [0.5, 80., 0.5],
        appearance: [0; 4],
        model: None,
        model_visual: None,
    }];
    let mut reads = TerrainReads::default();
    let mut requested = vec![];
    let plan = crate::server::gameplay::plan_with_lifecycles(
        &mut state.world,
        &mut reads,
        &mut requested,
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
                arguments: vec![3],
            }),
        },
        Participants {
            spawn_anchor: None,
            actor_inventory_revision: None,
            profile_inventories: None,
            profile_services: Some(&state.system_runtime),
            player_modifiers: None,
            players: &players,
            action_id: None,
            clock: None,
            weather: None,
            actor: Some((PROFILE, &before)),
            actor_position: Some([0.5, 80., 0.5]),
            admin: false,
            entities: &state.entities,
        },
        None,
    )
    .unwrap();
    let owner_changes = crate::server::players::state::prepare_writes(
        &state.system_runtime,
        &catalog,
        0,
        plan.profile_states,
    )
    .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: Some(PROFILE),
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: reads,
        inventory_before: Some(
            crate::inventory::InventoryStore::encode_snapshot_with_catalog(&before, &catalog)
                .unwrap(),
        ),
        inventory: plan.inventory,
        world_edits: vec![],
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entities: None,
        entity_wakes: vec![],
        owner_changes,
        sounds: vec![],
        player_publication: None,
    };
    assert!(
        state
            .durability
            .try_stage(crate::server::simulation::TickId::new(1), &action, None)
            .unwrap()
    );
    let (sender, receiver) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, receiver);
    real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    sender
        .send(Err(io::Error::other("injected health receipt failure")))
        .unwrap();
    assert!(complete_barrier(&mut state, CommitBarrier::AllStaged).is_err());
    assert!(state.durability.failed);
    assert_eq!(
        crate::server::players::health::view(&state.system_runtime, PROFILE)
            .unwrap()
            .current,
        100
    );
    assert!(
        state
            .inventory_store
            .load(PROFILE)
            .unwrap()
            .slots
            .iter()
            .all(Option::is_none)
    );
    drop(state);
    // The lost receipt hid an accepted WAL record. Recovery applies the whole
    // health + hook reward once, rather than publishing or re-running the hook.
    let state = fixture.open().unwrap();
    assert!(
        !crate::server::players::health::view(&state.system_runtime, PROFILE)
            .unwrap()
            .alive
    );
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}

#[test]
fn player_health_respawn_checkpoint_recovers_before_teleport_and_preserves_later_position() {
    use crate::server::durable::{CommitAction, CommitBarrier, complete_barrier};
    use bloxgloom_host_api::player_health::{PROFILE_SYSTEM, State as Health};
    let fixture = fixture();
    let mut state = state(&fixture);
    for x in [4, 6] {
        for y in 80..=82 {
            state.world.edit(x, y, 0, AIR).unwrap();
        }
        state.world.edit(x, 79, 0, crate::world::STONE).unwrap();
    }
    state.position_store.save(PROFILE, [0.5, 80., 0.5]).unwrap();
    let mut cell =
        crate::server::players::health::capture_profile(&state.system_runtime, PROFILE).unwrap();
    cell.state.data = Health {
        life: 3,
        respawn: Some((3, [4.5, 80., 0.5])),
        ..Default::default()
    }
    .encode()
    .unwrap();
    let changes = crate::server::players::state::prepare_writes(
        &state.system_runtime,
        state.world.catalog(),
        0,
        std::collections::BTreeMap::from([((PROFILE_SYSTEM.into(), PROFILE), cell)]),
    )
    .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: vec![],
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entities: None,
        entity_wakes: vec![],
        owner_changes: changes,
        sounds: vec![],
        player_publication: None,
    };
    state
        .durability
        .try_stage(crate::server::simulation::TickId::new(1), &action, None)
        .unwrap();
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    // Stop at the exact crash window: committed owner bytes exist, while the
    // session-bound native teleport has not advanced the old position file.
    assert_eq!(
        state
            .position_store
            .load_with_life(PROFILE)
            .unwrap()
            .unwrap()
            .1,
        0
    );
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let _peer = Peer::connect(address, catalog);
        let store =
            crate::server::position_store::PositionStore::new(&fixture.0.join("save")).unwrap();
        assert_eq!(
            store.load_with_life(PROFILE).unwrap(),
            Some(([4.5, 80., 0.5], 3))
        );
    });
    let store = crate::server::position_store::PositionStore::new(&fixture.0.join("save")).unwrap();
    store.save(PROFILE, [6.5, 80., 0.5]).unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let _peer = Peer::connect(address, catalog);
        assert_eq!(
            store.load_with_life(PROFILE).unwrap(),
            Some(([6.5, 80., 0.5], 3))
        );
    });
}

#[test]
fn player_health_only_cross_target_death_defers_replacement_join_until_receipt() {
    use crate::server::durable::CommitAction;
    use bloxgloom_host_api::player_health::{PROFILE_SYSTEM, State as Health};
    let fixture = fixture();
    let mut state = state(&fixture);
    // A returning target profile has no inventory or package lifecycle write.
    // The health change originates from another actor, so reserving that actor's
    // inventory cannot accidentally protect the target's reconnect.
    const ACTOR: u128 = PROFILE + 1;
    state.position_store.save(PROFILE, [0.5, 80., 0.5]).unwrap();
    let mut cell =
        crate::server::players::health::capture_profile(&state.system_runtime, PROFILE).unwrap();
    cell.state.data = Health {
        current: 0,
        life: 2,
        ..Default::default()
    }
    .encode()
    .unwrap();
    let owner_changes = crate::server::players::state::prepare_writes(
        &state.system_runtime,
        state.world.catalog(),
        0,
        std::collections::BTreeMap::from([((PROFILE_SYSTEM.into(), PROFILE), cell)]),
    )
    .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: Some(ACTOR),
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        terrain_reads: Default::default(),
        inventory_before: None,
        inventory: None,
        world_edits: vec![],
        deltas: vec![],
        changed_cells: vec![],
        pickups: vec![],
        fire_seed: None,
        clock_change: None,
        weather_change: None,
        entities: None,
        entity_wakes: vec![],
        owner_changes,
        sounds: vec![],
        player_publication: None,
    };
    assert!(
        state
            .durability
            .try_stage(crate::server::simulation::TickId::new(1), &action, None)
            .unwrap()
    );
    let (release, held) = std::sync::mpsc::channel();
    let real = std::mem::replace(&mut state.durability.pending[0].receiver, held);
    let receipt = real.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    assert!(state.durability.profile_reserved(PROFILE));
    assert!(!state.durability.profile_reserved(ACTOR));
    assert!(
        crate::server::players::health::view(&state.system_runtime, PROFILE)
            .unwrap()
            .alive,
        "owner state changed before its receipt was released"
    );
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut stream,
            &ClientMessage::Hello {
                name: "health-replacement".into(),
                profile: PROFILE,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let (fingerprint, _) = receive_content_manifest(&mut stream);
        assert_eq!(fingerprint, catalog.fingerprint());
        protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint }).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let error = stream.peek(&mut [0; 1]).unwrap_err();
        assert!(
            matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),
            "held health receipt disconnected replacement instead of deferring admission: {error}"
        );
        release.send(Ok(receipt)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut welcomed = false;
        loop {
            match protocol::read_server_with_catalog(&mut stream, &catalog).unwrap() {
                ServerMessage::Welcome { .. } => welcomed = true,
                ServerMessage::PlayerHealth {
                    profile, health, ..
                } => {
                    assert!(welcomed, "health arrived before replacement admission");
                    assert_eq!(profile, PROFILE);
                    assert_eq!((health.current, health.life, health.revision), (0, 2, 1));
                    assert!(!health.alive, "replacement cached pre-receipt alive state");
                    break;
                }
                _ => {}
            }
        }
    });
}
