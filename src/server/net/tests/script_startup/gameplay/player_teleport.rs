//! Teleport validates final authoritative terrain and fences old TCP movement.
use super::*;

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.action("return function(h) h.register_action('demo:shift',1,'Teleport','empty',nil,'demo:action') end",r#"
        return function(c,e)
            local me=c.player_by_profile(c.player_profile)
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            local mode=string.byte(e.arguments,1)
            if mode==1 then
                c.teleport_player(me.session,2.5,80,0.5)
                c.set_block(2,80,0,'bloxgloom:stone')
            elseif mode==2 then
                c.teleport_player(me.session,2.5,80,0.5)
                error('reject teleport')
            elseif mode==3 then
                pcall(function() c.teleport_player(me.session,math.huge,300,0.5) end)
            elseif mode==4 then
                pcall(function() c.teleport_player(tostring(me.session),1200.5,300,0.5) end)
            else
                c.teleport_player(me.session,1200.5,300,0.5)
                assert(c.player_by_session(me.session).position[1]==1200.5)
            end
        end
    "#);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(manifest, format!("{text}requires bloxgloom:players/v1\n")).unwrap();
    fixture
}

#[test]
fn luau_player_teleport_loads_far_terrain_rejects_final_obstructions_and_resets_movement() {
    let fixture = fixture();
    let mut state = Box::new(fixture.open().unwrap());
    state.world.edit(2, 80, 0, AIR).unwrap();
    let far = crate::world::world_to_chunk(1200, 300, 0).0;
    assert!(state.world.cached_version(far).is_none());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for mode in 1..=4 {
            let request = peer.request(mode);
            assert!(!peer.send(&request).0);
        }
        let request = peer.request(0);
        let ClientMessage::EntityInteract { action_id, .. } = &request else {
            unreachable!()
        };
        protocol::write_client(&mut peer.stream, &request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut teleport = None;
        loop {
            match peer.read(deadline) {
                ServerMessage::PlayerTeleport {
                    profile,
                    session,
                    reset,
                    position,
                } => {
                    assert_eq!((profile, session), (PROFILE, peer.epoch));
                    assert_eq!(position, [1200.5, 300.0, 0.5]);
                    teleport = Some(reset);
                }
                ServerMessage::ActionResult {
                    action_id: id,
                    accepted,
                    reason,
                } if id == *action_id => {
                    assert!(accepted, "{reason}");
                    break;
                }
                _ => {}
            }
        }
        let reset = teleport.expect("accepted teleport did not reset its target");
        peer.inventory_at(1);
        protocol::write_client(
            &mut peer.stream,
            &ClientMessage::Move {
                seq: 1,
                dx: 100.0,
                dy: 0.0,
                dz: 0.0,
            },
        )
        .unwrap();
        protocol::write_client(&mut peer.stream, &ClientMessage::Ping { nonce: 71 }).unwrap();
        loop {
            match peer.read(deadline) {
                ServerMessage::Position { .. } => panic!("old input escaped teleport fence"),
                ServerMessage::Pong { nonce: 71 } => break,
                _ => {}
            }
        }
        protocol::write_client(
            &mut peer.stream,
            &ClientMessage::MovementReady {
                session: peer.epoch,
                reset,
                next_seq: 2,
            },
        )
        .unwrap();
        protocol::write_client(
            &mut peer.stream,
            &ClientMessage::Move {
                seq: 2,
                dx: 0.1,
                dy: 0.0,
                dz: 0.0,
            },
        )
        .unwrap();
        loop {
            if let ServerMessage::Position {
                ack_seq: 2,
                x,
                y,
                z,
            } = peer.read(deadline)
            {
                assert!((x - 1200.6).abs() < 0.001);
                assert_eq!((y, z), (300.0, 0.5));
                break;
            }
        }
        assert!(peer.send(&request).0, "receipt replay failed");
    });
    let state = fixture.open().unwrap();
    assert_eq!(state.world.cached_block(2, 80, 0), Some(AIR));
    let saved = state.position_store.load(PROFILE).unwrap().unwrap();
    assert!((saved[0] - 1200.6).abs() < 0.001);
    assert_eq!(
        state.inventory_store.load(PROFILE).unwrap().slots[0]
            .as_ref()
            .unwrap()
            .count,
        1
    );
    let state = Box::new(state);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut stream,
            &ClientMessage::Hello {
                name: "rejoined".into(),
                profile: PROFILE,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let (fingerprint, _) = receive_content_manifest(&mut stream);
        protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint }).unwrap();
        loop {
            if let ServerMessage::Position { x, y, z, .. } =
                protocol::read_server(&mut stream).unwrap()
            {
                assert_eq!([x, y, z], saved);
                break;
            }
        }
    });
}

#[test]
fn luau_player_teleport_clears_actual_client_prediction_before_resuming_movement() {
    let fixture = fixture();
    let state = Box::new(fixture.open().unwrap());
    serve(state, |address| {
        crate::client::exercise_player_teleport(
            &address.to_string(),
            fixture.0.join("unused-client-config"),
        )
    });
}
