//! Real native menu request -> authoritative movement -> client reconciliation.
use super::*;

#[test]
fn admin_flying_toggle_walks_jumps_falls_and_rejects_non_admin_over_real_listener() {
    let fixture = Fixture::new();
    let mut state = Box::new(crate::server::server_state(7, fixture.0.join("save")).unwrap());
    state.admin_profile = Some(0x5c71);
    state.spawn_anchor = [0.5, 85.0, 0.5];
    for x in -3..=3 {
        for z in -3..=3 {
            for y in 79..=90 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 79 || y == 84 {
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
        let mut other = TcpStream::connect(address).unwrap();
        other
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut other,
            &ClientMessage::Hello {
                name: "non-admin-flight".into(),
                profile: 0xBAD,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        complete_content_handshake(&mut other);
        while !matches!(
            protocol::read_server(&mut other).unwrap(),
            ServerMessage::WorldTime { .. }
        ) {}
        protocol::write_client(&mut other, &ClientMessage::SetFlying { flying: false }).unwrap();
        loop {
            if let ServerMessage::FlyingMode { flying } = protocol::read_server(&mut other).unwrap()
            {
                assert!(flying, "non-admin mode request must be rejected");
                break;
            }
        }
        other.shutdown(std::net::Shutdown::Both).unwrap();
        crate::client::exercise_player_flight(&address.to_string(), fixture.0.join("client.cfg"));
    });
}
