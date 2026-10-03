//! Effective rates and their ordered input fence over the real TCP listener.
use super::*;

#[test]
fn player_modifier_tcp_fences_old_inputs_and_restores_rates_after_removal() {
    let fixture = Fixture::new();
    fixture.action("return function(h) h.register_action('demo:shift',1,'Rates','empty',nil,'demo:action') end",r#"
        return function(c,e)
            local me=c.player_by_profile(c.player_profile)
            if string.byte(e.arguments,1)==0 then
                c.set_player_modifier(me.session,'demo:slow',{speed=0.5})
            else c.remove_player_modifier(me.session,'demo:slow') end
        end
    "#);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let mut source = std::fs::read_to_string(&manifest).unwrap();
    source.push_str("requires bloxgloom:players/v1\n");
    std::fs::write(manifest, source).unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let deadline = Instant::now() + Duration::from_secs(10);
        peer.write(&ClientMessage::Move {
            seq: 1,
            dx: 0.0,
            dy: 0.0,
            dz: 0.0,
        });
        let original = loop {
            if let ServerMessage::Position {
                ack_seq: 1,
                x,
                y,
                z,
            } = peer.read(deadline)
            {
                break [x, y, z];
            }
        };
        let request = peer.request(0);
        peer.write(&request);
        let (session, reset) = loop {
            if let ServerMessage::PlayerModifiers {
                movement,
                session,
                reset,
                position,
                ..
            } = peer.read(deadline)
            {
                assert_eq!(movement.speed, 0.5);
                assert_eq!(position, original);
                break (session, reset);
            }
        };
        // Neither unacknowledged input nor an old barrier can open movement.
        peer.write(&ClientMessage::Move {
            seq: 2,
            dx: 0.05,
            dy: 0.0,
            dz: 0.0,
        });
        peer.write(&ClientMessage::MovementReady {
            session,
            reset: reset + 1,
            next_seq: 3,
        });
        peer.write(&ClientMessage::Move {
            seq: 3,
            dx: 0.05,
            dy: 0.0,
            dz: 0.0,
        });
        peer.write(&ClientMessage::Ping { nonce: 91 });
        loop {
            match peer.read(deadline) {
                ServerMessage::Pong { nonce: 91 } => break,
                ServerMessage::Position { .. } => {
                    panic!("input crossed unacknowledged modifier fence")
                }
                _ => {}
            }
        }
        peer.write(&ClientMessage::MovementReady {
            session,
            reset,
            next_seq: 4,
        });
        peer.write(&ClientMessage::Move {
            seq: 4,
            dx: 0.05,
            dy: 0.0,
            dz: 0.0,
        });
        let moved = loop {
            if let ServerMessage::Position {
                ack_seq: 4,
                x,
                y,
                z,
            } = peer.read(deadline)
            {
                break [x, y, z];
            }
        };
        assert!((moved[0] - original[0] - 0.05).abs() < 0.001);
        let request = peer.request(1);
        peer.write(&request);
        let reset2 = loop {
            if let ServerMessage::PlayerModifiers {
                movement,
                reset: next,
                position,
                ..
            } = peer.read(deadline)
            {
                assert_eq!(movement, Default::default());
                assert_eq!(position, moved);
                assert!(next > reset);
                break next;
            }
        };
        peer.write(&ClientMessage::MovementReady {
            session,
            reset,
            next_seq: 5,
        });
        peer.write(&ClientMessage::Move {
            seq: 5,
            dx: 0.05,
            dy: 0.0,
            dz: 0.0,
        });
        peer.write(&ClientMessage::Ping { nonce: 92 });
        loop {
            match peer.read(deadline) {
                ServerMessage::Pong { nonce: 92 } => break,
                ServerMessage::Position { .. } => panic!("old modifier ack reopened movement"),
                _ => {}
            }
        }
        peer.write(&ClientMessage::MovementReady {
            session,
            reset: reset2,
            next_seq: 6,
        });
        peer.write(&ClientMessage::Move {
            seq: 6,
            dx: 0.05,
            dy: 0.0,
            dz: 0.0,
        });
        loop {
            if let ServerMessage::Position { ack_seq: 6, x, .. } = peer.read(deadline) {
                assert!((x - moved[0] - 0.05).abs() < 0.001);
                break;
            }
        }
    });
}
