//! Production nonblocking listener, durable health, exact-session decisions and hooks.
use super::*;
use bloxgloom_host_api::player_health::View;
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.package("demo","requires bloxgloom:actions/v1\nrequires bloxgloom:players/v1\nmodule action action.luau\nmodule hooks hooks.luau\nmodule policy policy.luau",r#"return function(h)
        h.register_action('demo:shift',1,'Health','empty',nil,'demo:action')
        h.register_damage_policy('demo:policy',1,'demo:policy')
        h.register_health_hook('demo:health',1,'demo:hooks')
    end"#);
    std::fs::write(fixture.0.join("packages/demo/action.luau"),r#"return function(c,e)
        local me=c.player_by_profile(c.player_profile)
        local health=c.player_health(me.session)
        assert(not pcall(function() health.current=999 end))
        local mode=string.byte(e.arguments,1)
        if mode==0 then c.damage_player(me.session,health.revision,30,'demo:test')
        elseif mode==1 then c.heal_player(me.session,health.revision,10)
        elseif mode==2 then c.set_player_max_health(me.session,health.revision,120)
        elseif mode==3 then c.damage_player(me.session,health.revision,1000000,'demo:test')
        elseif mode==8 then c.damage_player(me.session,health.revision,100,'demo:cancel')
        else
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if mode==4 then pcall(function() c.damage_player(me.session,health.revision,10,'foreign:test') end)
            elseif mode==5 then pcall(function() c.heal_player(me.session,health.revision,0) end)
            elseif mode==6 then c.damage_player(me.session,health.revision,10,'demo:test'); error('rollback damage')
            elseif mode==7 then pcall(function() c.damage_player(me.session,tostring(health.revision),10,'demo:test') end)
            elseif mode==9 then pcall(function() c.profile_state('bloxgloom:player_health',me.profile) end)
            end
        end
    end"#).unwrap();
    std::fs::write(fixture.0.join("packages/demo/policy.luau"),"return function(e) assert(e.give==nil); assert(not pcall(function() e.amount=1 end)); return if e.cause=='demo:cancel' then 0 else e.amount end").unwrap();
    std::fs::write(fixture.0.join("packages/demo/hooks.luau"),r#"return function(c,e)
        assert(c.player_health(e.player.session).alive==e.health.alive)
        if e.kind=='died' then assert(c.give('player',{item='bloxgloom:stick',count=1})); c.message_player(e.player.session,'Died once')
        elseif e.kind=='respawned' then c.message_player(e.player.session,'Respawned') end
    end"#).unwrap();
    fixture
}
fn state(fixture: &Fixture) -> Box<State> {
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80., 0.5];
    for y in 80..=82 {
        state.world.edit(0, y, 0, AIR).unwrap();
    }
    state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
    state
}
fn action_id(message: &ClientMessage) -> u128 {
    match message {
        ClientMessage::PlayerIntent { message, .. } => action_id(message),
        ClientMessage::EntityInteract { action_id, .. } => *action_id,
        _ => panic!("not an action"),
    }
}
fn send(
    peer: &mut Peer,
    request: &ClientMessage,
    expected: Option<(u32, u64)>,
) -> (bool, Option<View>, Option<u64>) {
    peer.write(request);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut result = None;
    let mut health = None;
    let mut reset = None;
    loop {
        match peer.read(deadline) {
            ServerMessage::ActionResult {
                action_id: got,
                accepted,
                reason,
            } if got == action_id(request) => {
                if !accepted {
                    eprintln!("health action rejected: {reason}");
                }
                result = Some(accepted);
            }
            ServerMessage::PlayerHealth { health: value, .. } => health = Some(value),
            ServerMessage::PlayerTeleport { reset: value, .. } => reset = Some(value),
            _ => {}
        }
        if let Some(accepted) = result
            && (!accepted
                || expected.is_none_or(|(hp, life)| {
                    health.is_some_and(|h| h.current == hp && h.life == life)
                }))
        {
            return (accepted, health, reset);
        }
    }
}
fn respawn(peer: &mut Peer, health: View) -> ClientMessage {
    let payload = Request {
        key: crate::gameplay::respawn::KEY.into(),
        version: 1,
        slot: 0,
        inventory_revision: peer.inventory.revision,
        entity: 0,
        entity_revision: 0,
        arguments: health.revision.to_le_bytes().to_vec(),
    }
    .encode()
    .unwrap();
    ClientMessage::PlayerIntent {
        life: health.life,
        message: Box::new(ClientMessage::EntityInteract {
            action_id: peer.next_id(),
            target: [0; 3],
            payload,
        }),
    }
}
#[test]
fn player_health_damage_heal_death_replay_and_manual_respawn_survive_restart() {
    let fixture = fixture();
    let mut killed = None;
    let mut death = None;
    let initial = state(&fixture);
    let catalog = initial.world.catalog_arc();
    serve(initial, |address| {
        let mut peer = Peer::connect(address, catalog);
        for mode in [4, 5, 6, 7, 9] {
            let request = peer.request(mode);
            assert!(
                !send(&mut peer, &request, None).0,
                "caught-invalid mode {mode} accepted"
            );
        }
        assert!(peer.inventory.slots.iter().all(Option::is_none));
        let cancelled = peer.request(8);
        assert!(!send(&mut peer, &cancelled, None).0);
        for (mode, hp) in [(0, 70), (1, 80), (2, 80)] {
            let request = peer.request(mode);
            let (accepted, health, _) = send(&mut peer, &request, Some((hp, 1)));
            assert!(accepted);
            assert_eq!(health.unwrap().max, if mode == 2 { 120 } else { 100 });
        }
        let request = peer.request(3);
        let (accepted, health, _) = send(&mut peer, &request, Some((0, 2)));
        assert!(accepted);
        death = health;
        peer.inventory_at(1);
        assert!(
            send(&mut peer, &request, None).0,
            "receipt replay lost accepted result"
        );
        let blocked = peer.request(0);
        assert!(!send(&mut peer, &blocked, None).0, "dead player acted");
        killed = Some(request);
    });
    let reopened = state(&fixture);
    let saved = crate::server::players::health::view(&reopened.system_runtime, PROFILE).unwrap();
    assert_eq!(saved, death.unwrap());
    assert!(!saved.alive);
    let catalog = reopened.world.catalog_arc();
    serve(reopened, |address| {
        let mut peer = Peer::connect(address, catalog);
        assert!(
            !send(&mut peer, killed.as_ref().unwrap(), None).0,
            "old session acted"
        );
        let request = respawn(&mut peer, saved);
        let (accepted, health, reset) = send(&mut peer, &request, Some((120, 3)));
        assert!(accepted);
        let health = health.unwrap();
        assert!(health.alive);
        assert_eq!(health.revision, saved.revision + 1);
        assert!(send(&mut peer, &request, None).0, "respawn replay failed");
        let raw = peer.request(0);
        assert!(
            !send(&mut peer, &raw, None).0,
            "old-life action accepted after respawn"
        );
        if let Some(reset) = reset {
            peer.write(&ClientMessage::MovementReady {
                session: peer.epoch,
                reset,
                next_seq: 2,
            });
        }
        let current = ClientMessage::PlayerIntent {
            life: 3,
            message: Box::new(peer.request(0)),
        };
        assert!(send(&mut peer, &current, Some((90, 3))).0);
    });
    let reopened = fixture.open().unwrap();
    let health = crate::server::players::health::view(&reopened.system_runtime, PROFILE).unwrap();
    assert_eq!((health.current, health.max, health.life), (90, 120, 3));
    assert_eq!(
        reopened.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}
