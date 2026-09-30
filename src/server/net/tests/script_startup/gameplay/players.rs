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
    assert_eq!(startup.catalog().player_lifecycles().count(), 1);
}
