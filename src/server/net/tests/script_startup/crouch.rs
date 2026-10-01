//! Authoritative posture through the production nonblocking listener.
use super::*;

fn connect(address: std::net::SocketAddr, profile: u128) -> (TcpStream, u64) {
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "crouch-test".into(),
            profile,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    complete_content_handshake(&mut peer);
    let mut entity = None;
    loop {
        match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::OwnedEntity { id } => entity = Some(id),
            ServerMessage::WorldTime { .. } => return (peer, entity.unwrap()),
            _ => {}
        }
    }
}

fn stance(peer: &mut TcpStream, expected_id: u64, expected: bool) {
    loop {
        if let ServerMessage::PlayerStance {
            entity_id,
            crouching,
        } = protocol::read_server(&mut *peer).unwrap()
            && entity_id == expected_id
            && crouching == expected
        {
            break;
        }
    }
}

fn movement(peer: &mut TcpStream, seq: u64, delta: [f32; 3]) -> [f32; 3] {
    protocol::write_client(
        &mut *peer,
        &ClientMessage::Move {
            seq,
            dx: delta[0],
            dy: delta[1],
            dz: delta[2],
        },
    )
    .unwrap();
    loop {
        if let ServerMessage::Position { ack_seq, x, y, z } =
            protocol::read_server(&mut *peer).unwrap()
            && ack_seq == seq
        {
            return [x, y, z];
        }
    }
}

#[test]
fn crouch_loopback_cannot_stand_in_ceiling_and_late_join_observes_posture() {
    let fixture = Fixture::new();
    let mut state = Box::new(crate::server::server_state(7, fixture.0.join("save")).unwrap());
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=4 {
        for z in -1..=2 {
            for y in 79..=83 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 79 || (x == 0 && z == 0 && y == 82) {
                            crate::world::STONE
                        } else {
                            crate::world::AIR
                        },
                    )
                    .unwrap();
            }
        }
    }
    gameplay::serve(state, |address| {
        let (mut actor, entity) = connect(address, 0xC701);
        protocol::write_client(&mut actor, &ClientMessage::SetCrouching { crouching: true })
            .unwrap();
        stance(&mut actor, entity, true);
        let raised = movement(&mut actor, 1, [0.0, 0.5, 0.0]);
        assert_eq!(raised, [0.5, 80.5, 0.5]);
        protocol::write_client(
            &mut actor,
            &ClientMessage::SetCrouching { crouching: false },
        )
        .unwrap();
        // A new peer snapshots the retained authoritative posture, even after
        // the crouching player has released the key under a low ceiling.
        let (mut observer, _) = connect(address, 0xC702);
        stance(&mut observer, entity, true);
        let still_under = movement(&mut actor, 2, [0.1, 0.0, 0.0]);
        assert_eq!(still_under[1], 80.5);
        // The outstanding stand request succeeds automatically when movement
        // leaves the roof; observers receive the authoritative transition.
        for seq in 3..=8 {
            movement(&mut actor, seq, [0.25, 0.0, 0.0]);
        }
        stance(&mut observer, entity, false);
        protocol::write_client(&mut actor, &ClientMessage::SetCrouching { crouching: true })
            .unwrap();
        stance(&mut observer, entity, true);
        actor.shutdown(std::net::Shutdown::Both).unwrap();
        stance(&mut observer, entity, false);
    });
}
