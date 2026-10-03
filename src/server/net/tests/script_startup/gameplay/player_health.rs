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
        elseif mode==10 then c.damage_player(me.session,health.revision,1000000,'demo:test'); c.respawn_player(me.session,health.revision,4.5,80,0.5)
        else
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if mode==4 then pcall(function() c.damage_player(me.session,health.revision,10,'foreign:test') end)
            elseif mode==5 then pcall(function() c.heal_player(me.session,health.revision,0) end)
            elseif mode==6 then c.damage_player(me.session,health.revision,10,'demo:test'); error('rollback damage')
            elseif mode==7 then pcall(function() c.damage_player(me.session,tostring(health.revision),10,'demo:test') end)
            elseif mode==9 then pcall(function() c.profile_state('bloxgloom:player_health',me.profile) end)
            elseif mode==11 then pcall(function() c.damage_player(me.session,health.revision,10,'demo:invalid') end)
            end
        end
    end"#).unwrap();
    std::fs::write(fixture.0.join("packages/demo/policy.luau"),"return function(e) assert(e.give==nil); assert(not pcall(function() e.amount=1 end)); return if e.cause=='demo:cancel' then 0 elseif e.cause=='demo:invalid' then 1000001 else e.amount end").unwrap();
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
        ClientMessage::EntityInteract { action_id, .. }
        | ClientMessage::DropStack { action_id, .. } => *action_id,
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
        for mode in [4, 5, 6, 7, 9, 11] {
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
        let drop = ClientMessage::PlayerIntent {
            life: 2,
            message: Box::new(ClientMessage::DropStack {
                action_id: peer.next_id(),
                slot: 0,
                count: 1,
            }),
        };
        assert!(
            !send(&mut peer, &drop, None).0,
            "dead player dropped inventory"
        );
        for life in [1, 2] {
            peer.write(&ClientMessage::PlayerIntent {
                life,
                message: Box::new(ClientMessage::Move {
                    seq: 91,
                    dx: 50.,
                    dy: 10.,
                    dz: 0.,
                }),
            });
        }
        peer.write(&ClientMessage::Ping { nonce: 921 });
        loop {
            match peer.read(Instant::now() + Duration::from_secs(10)) {
                ServerMessage::Position { .. } => panic!("dead movement produced position"),
                ServerMessage::Pong { nonce: 921 } => break,
                _ => {}
            }
        }
        assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 1);

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

#[path = "player_health/audio.rs"]
mod audio;
#[path = "player_health/transactions.rs"]
mod transactions;
#[path = "player_health/transitions.rs"]
mod transitions;

#[test]
fn player_health_respawn_hook_final_collision_rolls_back_health_reward_and_terrain() {
    let fixture = fixture();
    let initial = state(&fixture);
    let catalog = initial.world.catalog_arc();
    serve(initial, |address| {
        let mut peer = Peer::connect(address, catalog);
        let kill = peer.request(3);
        assert!(send(&mut peer, &kill, Some((0, 2))).0);
        peer.inventory_at(1);
    });
    std::fs::write(
        fixture.0.join("packages/demo/hooks.luau"),
        r#"return function(c,e)
        assert(e.kind=='respawned','wrong health transition')
        if e.kind=='respawned' then
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            c.set_block(0,80,0,'bloxgloom:stone')
        end
    end"#,
    )
    .unwrap();
    let reopened = state(&fixture);
    assert_eq!(reopened.world.catalog().health_hooks().count(), 1);
    let health = crate::server::players::health::view(&reopened.system_runtime, PROFILE).unwrap();
    let catalog = reopened.world.catalog_arc();
    serve(reopened, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = respawn(&mut peer, health);
        assert!(!send(&mut peer, &request, None).0);
    });
    let mut reopened = fixture.open().unwrap();
    assert_eq!(
        crate::server::players::health::view(&reopened.system_runtime, PROFILE).unwrap(),
        health
    );
    assert_eq!(reopened.world.get_block(0, 80, 0).unwrap(), AIR);
    assert_eq!(
        reopened.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
}
#[test]
fn player_health_declarations_require_owned_modules_capability_and_positive_revisions() {
    for bad in [
        "h.register_health_hook('foreign:health',1,'demo:hooks')",
        "h.register_damage_policy('demo:damage',0,'demo:policy')",
        "h.register_health_hook('demo:health',1,'demo:missing')",
        "pcall(function() h.register_damage_policy('demo:damage',1,{}) end)",
    ] {
        let fixture = fixture();
        std::fs::write(
            fixture.0.join("packages/demo/main.luau"),
            format!("return function(h) {bad} end"),
        )
        .unwrap();
        assert!(
            fixture.open().is_err(),
            "invalid declaration accepted: {bad}"
        );
        assert!(!fixture.0.join("save").exists());
    }
    let fixture = fixture();
    let path = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("requires bloxgloom:players/v1\n", "");
    std::fs::write(path, text).unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn player_health_playable_package_commands_use_production_listener() {
    let fixture = Fixture::new();
    let package = fixture.0.join("packages/demo");
    std::fs::create_dir(&package).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/player-health/packages/demo");
    for name in [
        "package.txt",
        "main.luau",
        "action.luau",
        "damage.luau",
        "hooks.luau",
    ] {
        std::fs::copy(source.join(name), package.join(name)).unwrap();
    }
    let initial = state(&fixture);
    let catalog = initial.world.catalog_arc();
    serve(initial, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = ClientMessage::EntityInteract {
            action_id: peer.next_id(),
            target: [0; 3],
            payload: Request {
                key: "demo:hurt".into(),
                version: 1,
                slot: 0,
                inventory_revision: peer.inventory.revision,
                entity: 0,
                entity_revision: 0,
                arguments: vec![25],
            }
            .encode()
            .unwrap(),
        };
        assert!(send(&mut peer, &request, Some((75, 1))).0);
    });
}
