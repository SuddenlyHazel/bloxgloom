//! Exact online identities through real nonblocking admission and action dispatch.
use super::*;

#[test]
fn luau_player_directory_uses_claimed_profiles_and_exact_sessions_over_listener() {
    let fixture = Fixture::new();
    fixture.action(
        "return function(h) h.register_action('demo:shift',1,'Who','empty',nil,'demo:action') end",
        r#"
        return function(c,e)
            local list=c.players()
            assert(#list==1)
            local me=c.player_by_profile(c.player_profile)
            assert(me and me.profile==list[1].profile and me.name=='luau-action')
            assert(me.identity_trust=='claimed_profile' and me.online)
            assert(me.entity~=nil and c.player_by_session(me.session).profile==me.profile)
            assert(tostring(me.session):match('^session:'))
            assert(not pcall(function() me.position[1]=99 end))
            assert(not pcall(function() list[2]=me end))
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if string.byte(e.arguments,1)==1 then
                pcall(function() c.player_by_session(1) end)
            end
        end
    "#,
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let initial_revision = peer.inventory.revision;
        let query = peer.request(0);
        let (accepted, reason) = peer.send(&query);
        assert!(accepted, "{reason}");
        let deadline = Instant::now() + Duration::from_secs(10);
        while peer.inventory.revision == initial_revision {
            peer.read(deadline);
        }
        let before = peer.inventory.clone();
        let forged = peer.request(1);
        let (accepted, reason) = peer.send(&forged);
        assert!(
            !accepted && reason.contains("expected session ID"),
            "{reason}"
        );
        assert_eq!(
            peer.inventory, before,
            "caught forged identity published a grant"
        );
    });
}

impl Fixture {
    fn player_service(&self, source: &str) {
        self.package("demo", "requires bloxgloom:players/v1\nmodule player player.luau",
            "return function(h) h.register_player_lifecycle('demo:progress',1,64,'','demo:player') end");
        std::fs::write(self.0.join("packages/demo/player.luau"), source).unwrap();
    }
}

#[test]
fn player_public_projection_is_local_receipt_gated_and_recovers_over_listener() {
    let fixture = Fixture::new();
    fixture.player_service(r#"return function(c,e)
        if e.kind=='PlayerJoined' and e.state=='' then
            c.give('player',{item='bloxgloom:stick',count=1})
            return {state='SECRET:'..tostring(e.profile), public_state=string.char(0,255)..tostring(e.profile)}
        end
    end"#);
    let mut previous_epoch = 0;
    for _ in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, Arc::clone(&catalog));
            assert!(peer.epoch > previous_epoch);
            previous_epoch = peer.epoch;
            let expected = [&[0, 255][..], format!("profile:{PROFILE:032x}").as_bytes()].concat();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let ServerMessage::PlayerStates {
                    profile,
                    session,
                    snapshot,
                    states,
                } = peer.read(deadline)
                {
                    assert_eq!((profile, session), (PROFILE, peer.epoch));
                    assert!(snapshot > 0);
                    assert_eq!(states.len(), 1);
                    if states[0].revision > 0 {
                        assert_eq!(states[0].key, "demo:progress");
                        assert_eq!(states[0].public, expected);
                        break;
                    }
                }
            }
            peer.inventory_at(1);
            let other_profile = PROFILE + 1;
            let mut other = TcpStream::connect(address).unwrap();
            other
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut other,
                &ClientMessage::Hello {
                    name: "other".into(),
                    profile: other_profile,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut other);
            protocol::write_client(&mut other, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let other_public = [
                &[0, 255][..],
                format!("profile:{other_profile:032x}").as_bytes(),
            ]
            .concat();
            loop {
                if let ServerMessage::PlayerStates {
                    profile, states, ..
                } = protocol::read_server(&mut other).unwrap()
                {
                    assert_eq!(profile, other_profile);
                    if states[0].revision > 0 {
                        assert_eq!(states[0].public, other_public);
                        break;
                    }
                }
            }
        });
    }
}

#[test]
fn player_client_ready_updates_panel_and_retires_hooks_on_real_reconnect() {
    let fixture = Fixture::new();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/player-lifecycle/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&root)
        .unwrap();
    let state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    );
    serve(state, |address| {
        crate::client::exercise_player_services(
            &address.to_string(),
            fixture.0.join("unused-client-config"),
        )
    });
}
#[test]
fn luau_player_first_join_reward_state_and_session_survive_listener_restart() {
    use crate::server::{parallel::OwnerKey, registry::SystemId};
    use bloxgloom_host_api::players::State as ProfileState;
    let fixture = Fixture::new();
    fixture.player_service(
        r#"
        return function(c,e)
            assert(e.profile==c.player_profile and e.identity_trust=='claimed_profile')
            if e.kind=='PlayerJoined' then
                assert(e.session_state=='')
                if e.state=='' then
                    assert(c.give('player',{item='bloxgloom:stick',count=3}))
                    return {state='kit',public_state='level:1',session_state='connected'}
                end
                assert(e.state=='kit')
                return {session_state='connected'}
            elseif e.kind=='PlayerSpawned' then
                assert(e.state=='kit' and e.session_state=='connected')
            end
        end
    "#,
    );
    let system = SystemId::new("demo:progress").unwrap();
    for round in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        if round == 1 {
            let (_, saved) = state
                .system_runtime
                .owner_snapshot(&system, OwnerKey::Profile(PROFILE))
                .unwrap();
            assert_eq!(
                saved.get::<ProfileState>().unwrap(),
                &ProfileState {
                    data: b"kit".to_vec(),
                    public_data: b"level:1".to_vec()
                }
            );
        }
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            peer.inventory_at(3);
            // Read a later independent clock frame so queued spawn work gets a turn.
            let deadline = Instant::now() + Duration::from_secs(10);
            while !matches!(peer.read(deadline), ServerMessage::WorldTime { .. }) {}
        });
    }
    let state = fixture.open().unwrap();
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        3,
        "reconnect duplicated first-join reward"
    );
}

#[test]
fn luau_player_failed_join_reward_publishes_neither_flag_nor_inventory() {
    use crate::server::{parallel::OwnerKey, registry::SystemId};
    let fixture = Fixture::new();
    fixture.player_service(
        r#"return function(c,e)
        if e.kind=='PlayerJoined' then
            assert(c.give('player',{item='bloxgloom:stick',count=3}))
            return {state=string.rep('x',65)}
        end
    end"#,
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..2 {
            while !matches!(peer.read(deadline), ServerMessage::WorldTime { .. }) {}
        }
        assert!(peer.inventory.slots.iter().all(Option::is_none));
    });
    let state = fixture.open().unwrap();
    assert!(
        state
            .inventory_store
            .load(PROFILE)
            .unwrap()
            .slots
            .iter()
            .all(Option::is_none)
    );
    if let Some((_, cell)) = state.system_runtime.owner_snapshot(
        &SystemId::new("demo:progress").unwrap(),
        OwnerKey::Profile(PROFILE),
    ) {
        assert!(
            cell.get::<bloxgloom_host_api::players::State>()
                .unwrap()
                .data
                .is_empty()
        );
    }
}

#[test]
fn luau_player_profile_timer_runs_offline_on_logical_deadline_after_restart() {
    use crate::server::{
        durable::{CommitAction, CommitBarrier, complete_barrier},
        parallel::{OwnerData, OwnerKey},
        registry::SystemId,
        simulation::TickId,
    };
    use bloxgloom_host_api::players::State as ProfileState;
    let fixture = Fixture::new();
    fixture.player_service(
        r#"return function(c,e)
        assert(e.kind=='ProfileTick' and e.player==nil and e.state=='waiting')
        assert(c.give('player',{item='bloxgloom:stick',count=2}))
        return {state='done'}
    end"#,
    );
    let system = SystemId::new("demo:progress").unwrap();
    let mut state = fixture.open().unwrap();
    let change = state
        .system_runtime
        .stage_profile_insert(
            &system,
            PROFILE,
            &OwnerData::new(ProfileState {
                data: b"waiting".to_vec(),
                public_data: vec![],
            }),
            Some(1000),
        )
        .unwrap();
    let action = CommitAction {
        client_id: None,
        profile: Some(PROFILE),
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
        owner_changes: vec![change],
        player_publication: None,
    };
    assert!(
        state
            .durability
            .try_stage(TickId::new(1), &action, None)
            .unwrap()
    );
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    drop(state);
    let mut state = fixture.open().unwrap();
    crate::server::players::drive(&mut state, TickId::new(999)).unwrap();
    assert_eq!(
        state.system_runtime.profile_deadline(&system, PROFILE),
        Some(1000)
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
    crate::server::players::drive(&mut state, TickId::new(1000)).unwrap();
    complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    // Offline inventories load on workers; retry at the same logical tick so
    // file completion cannot consume extra game time or lose the due timer.
    let deadline = Instant::now() + Duration::from_secs(10);
    while state
        .system_runtime
        .profile_deadline(&system, PROFILE)
        .is_some()
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
        crate::server::players::drive(&mut state, TickId::new(1000)).unwrap();
        complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    }
    assert_eq!(
        state.system_runtime.profile_deadline(&system, PROFILE),
        None
    );
    let (_, cell) = state
        .system_runtime
        .owner_snapshot(&system, OwnerKey::Profile(PROFILE))
        .unwrap();
    assert_eq!(cell.get::<ProfileState>().unwrap().data, b"done");
    assert_eq!(
        state.durability.inventory_overlay[&PROFILE].slots[0]
            .as_ref()
            .unwrap()
            .count,
        2
    );
    let wal = crate::server::journal::Journal::open(fixture.0.join("save/server.wal")).unwrap();
    let record = wal.records().last().unwrap();
    assert!(
        record
            .changes
            .iter()
            .any(|c| c.key.domain == "bloxgloom:owner_state")
    );
    assert!(
        record
            .changes
            .iter()
            .any(|c| c.key.domain == "bloxgloom:inventory")
    );
}

#[test]
fn luau_player_cancelled_listener_join_never_grants_first_join_reward() {
    use crate::server::{parallel::OwnerKey, registry::SystemId};
    let fixture = Fixture::new();
    fixture.player_service(
        r#"return function(c,e)
        if e.kind=='PlayerJoining' then return {spawn={16000,80,0}} end
        if e.kind=='PlayerJoined' then
            c.give('player',{item='bloxgloom:stick',count=3})
            return {state='joined'}
        end
    end"#,
    );
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut socket,
            &ClientMessage::Hello {
                name: "cancelled".into(),
                profile: PROFILE,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let (fingerprint, _) = receive_content_manifest(&mut socket);
        protocol::write_client(&mut socket, &ClientMessage::ContentReady { fingerprint }).unwrap();
        socket.shutdown(std::net::Shutdown::Both).unwrap();
        drop(socket);
        std::thread::sleep(Duration::from_millis(200));
    });
    let state = fixture.open().unwrap();
    assert!(
        state
            .inventory_store
            .load(PROFILE)
            .unwrap()
            .slots
            .iter()
            .all(Option::is_none)
    );
    assert!(
        state
            .system_runtime
            .owner_snapshot(
                &SystemId::new("demo:progress").unwrap(),
                OwnerKey::Profile(PROFILE)
            )
            .is_none()
    );
}

#[test]
fn luau_player_session_timer_cancels_on_disconnect_and_reconnect_starts_empty() {
    use crate::server::{
        durable::{CommitBarrier, complete_barrier},
        simulation::TickId,
    };
    let fixture = Fixture::new();
    fixture.player_service(
        r#"return function(c,e)
        if e.kind=='PlayerJoined' then
            assert(e.session_state=='')
            return {state='joined',session_state='live',session_delay=10}
        elseif e.kind=='PlayerLeft' then
            assert(e.session_state=='live')
            return {state='left'}
        elseif e.kind=='SessionTick' then
            assert(c.give('player',{item='bloxgloom:stick',count=32}))
        end
    end"#,
    );
    let mut state = fixture.open().unwrap();
    fn session(
        state: &mut State,
        epoch: u64,
    ) -> (
        u64,
        TcpStream,
        std::sync::mpsc::Receiver<crate::server::outbound::OutboundFrame>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (socket, _) = listener.accept().unwrap();
        let (sender, receiver) = state.outbound.client_queue();
        let inventory = state
            .durability
            .inventory_overlay
            .get(&PROFILE)
            .cloned()
            .unwrap_or_else(|| state.inventory_store.load(PROFILE).unwrap());
        let id = crate::server::players::admission::join_named_client(
            state,
            PROFILE,
            "luau-action",
            epoch,
            inventory,
            sender,
            &socket,
        )
        .unwrap()
        .id;
        crate::server::players::joined(state, id);
        (id, peer, receiver)
    }
    let (id, _peer, _frames) = session(&mut state, 1);
    for tick in 1..=2 {
        crate::server::players::drive(&mut state, TickId::new(tick)).unwrap();
        complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    }
    assert_eq!(state.player_runtime.session_count(), 1);
    state.remove_client(id);
    assert_eq!(
        state.player_runtime.session_count(),
        0,
        "disconnect retained session participation state"
    );
    let (_id, _peer2, _frames2) = session(&mut state, 2);
    for tick in 3..=12 {
        crate::server::players::drive(&mut state, TickId::new(tick)).unwrap();
        complete_barrier(&mut state, CommitBarrier::AllStaged).unwrap();
    }
    assert!(
        state
            .clients
            .values()
            .next()
            .unwrap()
            .inventory
            .slots
            .iter()
            .all(Option::is_none),
        "old session deadline granted items to replacement connection"
    );
    assert_eq!(state.player_runtime.session_count(), 1);
}

#[test]
fn luau_player_lifecycle_example_uses_supported_server_only_package_format() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/player-lifecycle/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&root)
        .unwrap();
    let catalog = startup.catalog();
    let keys: std::collections::BTreeSet<_> = catalog
        .player_lifecycles()
        .map(|reg| reg.key.as_str())
        .collect();
    assert_eq!(
        keys,
        std::collections::BTreeSet::from(["welcome:policy", "welcome:progress"])
    );
}

#[test]
fn luau_player_command_roster_checks_target_session_before_handler_over_listener() {
    let fixture = Fixture::new();
    fixture.action("return function(h) h.register_action('demo:shift',1,'Target','empty',nil,'demo:action',{permission='Player',arguments={{kind='player'}}}) end",r#"return function(c,e)
        local target=c.player_by_session(e.command_arguments[1])
        assert(target and target.profile==c.player_profile)
        assert(not pcall(function() e.command_arguments[1]=1 end))
        assert(c.give('player',{item='bloxgloom:stick',count=1}))
    end"#);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog.clone());
        let deadline = Instant::now() + Duration::from_secs(10);
        let players = loop {
            if let ServerMessage::PlayerRoster { revision, players } = peer.read(deadline) {
                assert!(revision > 0);
                break players;
            }
        };
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].session, peer.epoch);
        let schema = catalog
            .action("demo:shift")
            .unwrap()
            .command
            .as_ref()
            .unwrap();
        let token = format!("session:{PROFILE:032x}:{:016x}", peer.epoch);
        fn request(peer: &mut Peer, arguments: Vec<u8>) -> ClientMessage {
            let mut request = peer.request(0);
            if let ClientMessage::EntityInteract { payload, .. } = &mut request {
                let mut value = Request::decode(payload).unwrap();
                value.arguments = arguments;
                *payload = value.encode().unwrap();
            }
            request
        }
        let accepted = request(&mut peer, schema.encode_arguments(&[&token]).unwrap());
        assert!(peer.send(&accepted).0);
        peer.inventory_at(1);
        let mut stale = schema.encode_arguments(&[&token]).unwrap();
        stale[16..24].copy_from_slice(&(peer.epoch + 1).to_le_bytes());
        let rejected = request(&mut peer, stale);
        let (accepted, reason) = peer.send(&rejected);
        assert!(!accepted && reason.contains("session is stale"), "{reason}");
        assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 1);
    });
}
