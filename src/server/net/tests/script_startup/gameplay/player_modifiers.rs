//! Modifier ownership, WAL rollback and lifetimes over the real reactor path.
use super::*;
use bloxgloom_host_api::player_modifiers::{PROFILE_SYSTEM, Set};

fn fixture(authority: bool) -> Fixture {
    let fixture = Fixture::new();
    fixture.action("return function(h) h.register_action('demo:shift',1,'Modifiers','empty',nil,'demo:action') end",r#"
        return function(c,e)
            local me=c.player_by_profile(c.player_profile)
            local mode=string.byte(e.arguments,1)
            if mode==4 then
                assert(#c.player_modifiers(me.session)==0)
                assert(#c.player_modifiers(me.profile)==1)
                c.message_player(me.session,'Profile effects survived; session effects cleared')
                return
            end
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if mode==5 then
                c.set_player_modifier(me.profile,'demo:short',{speed=0.5,duration_ticks=10})
                c.set_player_modifier(me.session,'demo:short',{speed=0.5,duration_ticks=10})
                return
            end
            c.set_player_modifier(me.profile,'demo:boost',{speed=1.25})
            c.set_player_modifier(me.session,'demo:slow',{speed=0.5})
            assert(#c.player_modifiers(me.profile)==1 and #c.player_modifiers(me.session)==1)
            assert(not pcall(function() c.player_modifiers(me.profile)[1].speed=4 end))
            if mode==1 then error('reject after modifier') end
            if mode==2 then pcall(function() c.set_player_modifier(me.profile,'foreign:boost',{speed=2}) end) end
            if mode==3 then pcall(function() c.set_player_modifier(me.session,'demo:bad',{gravity=0}) end) end
            if mode==6 then pcall(function() c.profile_state('bloxgloom:player_modifiers',me.profile) end) end
        end
    "#);
    if authority {
        let path = fixture.0.join("packages/demo/package.txt");
        std::fs::write(
            &path,
            format!(
                "{}requires bloxgloom:players/v1\n",
                std::fs::read_to_string(&path).unwrap()
            ),
        )
        .unwrap();
    }
    fixture
}
fn profile_set(state: &State) -> Set {
    let cell =
        crate::server::players::modifiers::capture_profile(&state.system_runtime, PROFILE).unwrap();
    Set::decode(&cell.state.data).unwrap()
}

#[test]
fn player_modifiers_require_owned_authority_and_commit_with_finite_inventory() {
    for authority in [false, true] {
        let fixture = fixture(authority);
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            for mode in [1, 2, 3, 6] {
                let request = peer.request(mode);
                assert!(!peer.send(&request).0);
            }
            let request = peer.request(0);
            assert_eq!(peer.send(&request).0, authority);
            if authority {
                peer.inventory_at(1);
                assert!(
                    peer.send(&request).0,
                    "receipt replay must return original result"
                );
            }
        });
        let state = fixture.open().unwrap();
        let set = profile_set(&state);
        assert_eq!(
            set.get("demo:boost").map(|e| e.movement.speed),
            authority.then_some(1.25)
        );
        assert!(
            set.get("demo:slow").is_none(),
            "session effect cannot enter saved profile state"
        );
        assert_eq!(
            state.inventory_store.load(PROFILE).unwrap().slots[0]
                .as_ref()
                .map(|s| s.count),
            authority.then_some(1)
        );
        assert!(
            state
                .player_modifiers
                .capture(PROFILE, 1)
                .iter()
                .next()
                .is_none()
        );
    }
}

#[test]
fn player_modifier_profile_survives_restart_but_session_and_old_epoch_do_not() {
    let fixture = fixture(true);
    let mut old_request = None;
    for round in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            if round == 0 {
                let request = peer.request(0);
                assert!(peer.send(&request).0);
                peer.inventory_at(1);
                old_request = Some(request);
            } else {
                assert!(
                    !peer.send(old_request.as_ref().unwrap()).0,
                    "departed-session request accepted"
                );
                let request = peer.request(4);
                let result = peer.send(&request);
                assert!(result.0, "{}", result.1);
            }
        });
        let state = fixture.open().unwrap();
        assert!(profile_set(&state).get("demo:boost").is_some());
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
fn player_modifier_expiry_is_checkpointed_while_profile_is_disconnected() {
    let fixture = fixture(true);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(5);
        let result = peer.send(&request);
        assert!(result.0, "{}", result.1);
        peer.inventory_at(1);
        peer.stream.shutdown(std::net::Shutdown::Both).unwrap();
        drop(peer);
        std::thread::sleep(Duration::from_millis(500));
    });
    let state = fixture.open().unwrap();
    let cell =
        crate::server::players::modifiers::capture_profile(&state.system_runtime, PROFILE).unwrap();
    assert!(
        Set::decode(&cell.state.data)
            .unwrap()
            .iter()
            .next()
            .is_none()
    );
    assert!(cell.initialized);
    assert_eq!(cell.next_tick, None);
    assert!(
        state
            .system_runtime
            .owner_snapshot(
                &crate::server::registry::SystemId::new(PROFILE_SYSTEM).unwrap(),
                crate::server::parallel::OwnerKey::Profile(PROFILE)
            )
            .is_some()
    );
}
