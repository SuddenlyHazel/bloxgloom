//! General profile transactions exercise the production nonblocking listener.
use super::*;
use crate::server::{parallel::OwnerKey, registry::SystemId};
use bloxgloom_host_api::players::State as ProfileState;
const OFFLINE: u128 = PROFILE + 1;
fn fixture(authority: bool) -> Fixture {
    let fixture = Fixture::new();
    fixture.package("demo", &format!("requires bloxgloom:actions/v1\n{}module action action.luau\nmodule policy policy.luau", if authority {"requires bloxgloom:players/v1\n"} else {""}),
        "return function(h) h.register_action('demo:shift',1,'Policy','empty',nil,'demo:action'); if h.register_player_lifecycle then h.register_player_lifecycle('demo:policy',1,64,'member','demo:policy') end end");
    // Lifecycle declaration also requires players/v1. A second owned package
    // provides a real foreign service for the denial tests.
    if !authority {
        std::fs::write(fixture.0.join("packages/demo/main.luau"), "return function(h) h.register_action('demo:shift',1,'Policy','empty',nil,'demo:action') end").unwrap();
    }
    std::fs::write(fixture.0.join("packages/demo/policy.luau"), r#"return function(c,e)
        if e.kind=='PlayerJoining' and e.state=='banned' then return {deny='Policy admission denied'} end
        if e.kind=='PlayerJoined' then return {profile_delay=100000} end
    end"#).unwrap();
    fixture.package("foreign", "requires bloxgloom:players/v1\nmodule policy policy.luau", "return function(h) h.register_player_lifecycle('foreign:policy',1,64,'secret','foreign:policy') end");
    std::fs::write(
        fixture.0.join("packages/foreign/policy.luau"),
        "return function() end",
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/action.luau"), format!(r#"return function(c,e)
        local mode=string.byte(e.arguments,1)
        local offline=c.profile_id('profile:{OFFLINE:032x}')
        assert(tostring(offline)=='profile:{OFFLINE:032x}')
        local me=c.profile_state('demo:policy',c.player_profile)
        assert(me.state=='member' or me.state=='role:builder')
        assert(me.next_tick~=nil)
        local old=c.profile_state('demo:policy',offline)
        assert(old.state=='member' or old.state=='banned')
        assert(not pcall(function() old.state='forged' end))
        if mode==9 then assert(me.state=='role:builder' and old.state=='banned'); c.message_player(c.player_by_profile(c.player_profile).session,'Policy checked'); return end
        c.set_profile_state('demo:policy',offline,'banned','')
        c.set_profile_state('demo:policy',c.player_profile,'role:builder',string.char(0,255)..'builder')
        local proposed=c.profile_state('demo:policy',c.player_profile)
        assert(proposed.revision==me.revision and proposed.next_tick==me.next_tick and proposed.state=='role:builder')
        if mode==8 then return end
        assert(c.give('player',{{item='bloxgloom:stick',count=1}}))
        if mode==10 then
            for i=1,65 do c.profile_state('demo:policy',c.profile_id('profile:0000000000000000000000000000'..string.format('%04x',i))) end
        end
        if mode==1 then error('reject profile update') end
        if mode==2 then pcall(function() c.profile_state('foreign:policy',offline) end) end
        if mode==3 then pcall(function() c.set_profile_state('demo:policy',offline,string.rep('x',65),'') end) end
        if mode==4 then pcall(function() c.profile_state('demo:policy',tostring(offline)) end) end
        if mode==5 then pcall(function() c.profile_id('profile:00000000000000000000000000000000') end) end
        if mode==6 then pcall(function() c.set_profile_state('demo:policy',offline,'',string.rep('x',1025)) end) end
        if mode==7 then pcall(function() c.profile_state('demo:missing',offline) end) end
    end"#)).unwrap();
    fixture
}
fn read_cell(state: &State, profile: u128) -> Option<(u64, ProfileState)> {
    state
        .system_runtime
        .owner_snapshot(
            &SystemId::new("demo:policy").unwrap(),
            OwnerKey::Profile(profile),
        )
        .map(|(revision, data)| (revision, data.get::<ProfileState>().unwrap().clone()))
}
#[test]
fn profile_state_actions_are_atomic_owned_binary_and_restart_safe_over_listener() {
    let fixture = fixture(true);
    let mut revision = 0;
    for restart in [false, true] {
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, Arc::clone(&catalog));
            // Joined scheduling precedes the action lane; ensure the public
            // snapshot represents a materialized policy cell before assertions.
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let ServerMessage::PlayerStates { states, .. } = peer.read(deadline)
                    && states
                        .iter()
                        .any(|s| s.key == "foreign:policy" && s.revision > 0)
                {
                    break;
                }
            }
            if !restart {
                for mode in (1..=7).chain([10]) {
                    let req = peer.request(mode);
                    assert!(!peer.send(&req).0, "mode {mode} accepted");
                }
                let req = peer.request(0);
                let ClientMessage::EntityInteract { action_id, .. } = &req else {
                    unreachable!()
                };
                protocol::write_client(&mut peer.stream, &req).unwrap();
                let mut result = false;
                let mut projection = false;
                while !result || !projection {
                    match peer.read(deadline) {
                        ServerMessage::ActionResult {
                            action_id: id,
                            accepted,
                            reason,
                        } if id == *action_id => {
                            assert!(accepted, "{reason}");
                            result = true;
                        }
                        ServerMessage::PlayerStates {
                            profile,
                            session,
                            states,
                            ..
                        } => {
                            assert_eq!((profile, session), (PROFILE, peer.epoch));
                            if let Some(cell) = states.iter().find(|s| {
                                s.key == "demo:policy"
                                    && s.public == [&[0, 255][..], b"builder"].concat()
                            }) {
                                revision = cell.revision;
                                projection = true;
                            }
                        }
                        _ => {}
                    }
                }
                assert!(peer.send(&req).0, "receipt replay rejected");
                peer.inventory_at(1);
                let pure_state = peer.request(8);
                let (accepted, reason) = peer.send(&pure_state);
                assert!(accepted, "state-only action: {reason}");
            } else {
                let req = peer.request(9);
                let (accepted, reason) = peer.send(&req);
                assert!(accepted, "restart query: {reason}");
            }
            peer.inventory_at(1);
            // A policy written while offline must govern that profile's next
            // actual handshake, and must not expose its private state to peers.
            let mut banned = TcpStream::connect(address).unwrap();
            banned
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut banned,
                &ClientMessage::Hello {
                    name: "offline".into(),
                    profile: OFFLINE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut banned);
            protocol::write_client(&mut banned, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let error = protocol::read_server(&mut banned).unwrap_err();
            assert!(
                matches!(
                    error.kind(),
                    io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset
                ),
                "{error}"
            );
        });
        let state = fixture.open().unwrap();
        let (current, cell) = read_cell(&state, PROFILE).unwrap();
        assert!(current >= revision);
        assert_eq!(cell.data, b"role:builder");
        assert_eq!(cell.public_data, [&[0, 255][..], b"builder"].concat());
        assert_eq!(read_cell(&state, OFFLINE).unwrap().1.data, b"banned");
        assert!(
            state
                .system_runtime
                .profile_deadline(&SystemId::new("demo:policy").unwrap(), PROFILE)
                .is_some()
        );
        assert_eq!(
            state.inventory_store.load(PROFILE).unwrap().slots[0]
                .as_ref()
                .unwrap()
                .count,
            1
        );
    }
}
#[test]
fn profile_state_service_requires_package_capability_over_listener() {
    let fixture = fixture(false);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let req = peer.request(0);
        let (accepted, reason) = peer.send(&req);
        assert!(
            !accepted && reason.contains("profile state owners"),
            "{reason}"
        );
        assert!(peer.inventory.slots.iter().all(Option::is_none));
    });
}

#[test]
fn profile_state_reads_reserve_existing_cells_and_absence_until_receipt() {
    use crate::server::durable::{CommitAction, StageError, TerrainReads};
    use crate::server::simulation::TickId;
    for existing in [false, true] {
        let fixture = fixture(true);
        let mut state = fixture.open().unwrap();
        let reg = state
            .world
            .catalog()
            .player_lifecycles()
            .find(|r| r.key == "demo:policy")
            .unwrap()
            .clone();
        let system = SystemId::new(&reg.key).unwrap();
        let value = ProfileState {
            data: b"member".to_vec(),
            public_data: vec![],
        };
        if existing {
            let changes = crate::server::players::state::prepare(
                &state.system_runtime,
                &reg,
                OFFLINE,
                value.clone(),
                None,
            )
            .unwrap();
            state
                .system_runtime
                .apply_replayed_owner_changes(&changes)
                .unwrap();
        }
        let revision = state
            .system_runtime
            .owner_snapshot(&system, OwnerKey::Profile(OFFLINE))
            .map(|(r, _)| r);
        let mut reads = TerrainReads::default();
        reads.profile(&system, OFFLINE, revision).unwrap();
        assert!(reads.profiles_current(&state.system_runtime));
        let action = |profile, reads| CommitAction {
            client_id: None,
            profile: None,
            action_id: None,
            receipt_value: None,
            receipt_transition: None,
            inventory_before: None,
            inventory: None,
            world_edits: vec![],
            terrain_reads: reads,
            deltas: vec![],
            changed_cells: vec![],
            pickups: vec![],
            fire_seed: None,
            clock_change: None,
            weather_change: None,
            entities: None,
            entity_wakes: vec![],
            sounds: Vec::new(),
            player_publication: None,
            owner_changes: crate::server::players::state::prepare(
                &state.system_runtime,
                &reg,
                profile,
                value.clone(),
                None,
            )
            .unwrap(),
        };
        let first = action(PROFILE + 10, reads.clone());
        let second = action(PROFILE + 11, reads.clone());
        let writer = action(OFFLINE, TerrainReads::default());
        assert!(
            state
                .durability
                .try_stage(TickId::new(1), &first, None)
                .unwrap()
        );
        assert!(
            state.durability.profile_reserved(OFFLINE),
            "admission ignored profile read reservation"
        );
        assert!(
            state
                .durability
                .try_stage(TickId::new(1), &second, None)
                .unwrap(),
            "shared profile readers conflicted"
        );
        assert!(matches!(
            state.durability.try_stage(TickId::new(1), &writer, None),
            Err(StageError::Conflict)
        ));
        // A changed revision must also be caught by the publication validation,
        // even if another native producer accidentally bypassed reservations.
        state
            .system_runtime
            .apply_replayed_owner_changes(&writer.owner_changes)
            .unwrap();
        assert!(!reads.profiles_current(&state.system_runtime));
        let mut stale = std::collections::BTreeMap::new();
        stale.insert(
            (reg.key.clone(), OFFLINE),
            bloxgloom_host_api::gameplay::ProfileCell {
                revision: revision.unwrap_or(0),
                initialized: revision.is_some(),
                state: value,
                next_tick: None,
            },
        );
        assert_eq!(
            crate::server::players::state::prepare_writes(
                &state.system_runtime,
                state.world.catalog(),
                0,
                stale
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn lifecycle_profile_writes_compose_and_ambiguous_decisions_rollback_over_listener() {
    for conflicting in [false, true] {
        let fixture = fixture(true);
        std::fs::write(fixture.0.join("packages/demo/policy.luau"), format!(r#"return function(c,e)
            if e.kind=='PlayerJoined' then
                c.set_profile_state('demo:policy',e.profile,'role:builder','builder')
                c.set_profile_state('demo:policy',c.profile_id('profile:{OFFLINE:032x}'),'banned','')
                assert(c.give('player',{{item='bloxgloom:stick',count=1}}))
                return {{profile_delay=100000{}}}
            end
        end"#, if conflicting {",state='second writer'"} else {""})).unwrap();
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            let deadline = Instant::now() + Duration::from_secs(10);
            // Spawned is queued after Joined, so its materialized snapshot
            // is a fence proving the attempted joined decision has finished.
            loop {
                if let ServerMessage::PlayerStates { states, .. } = peer.read(deadline)
                    && states
                        .iter()
                        .any(|s| s.key == "foreign:policy" && s.revision > 0)
                {
                    break;
                }
            }
            if !conflicting {
                peer.inventory_at(1);
            }
        });
        let state = fixture.open().unwrap();
        let cell = read_cell(&state, PROFILE).unwrap().1;
        if conflicting {
            assert_eq!(cell.data, b"member");
            assert!(read_cell(&state, OFFLINE).is_none());
            assert!(
                state
                    .inventory_store
                    .load(PROFILE)
                    .unwrap()
                    .slots
                    .iter()
                    .all(Option::is_none)
            );
        } else {
            assert_eq!(cell.data, b"role:builder");
            assert_eq!(read_cell(&state, OFFLINE).unwrap().1.data, b"banned");
            assert_eq!(
                state.inventory_store.load(PROFILE).unwrap().slots[0]
                    .as_ref()
                    .unwrap()
                    .count,
                1
            );
        }
    }
}
