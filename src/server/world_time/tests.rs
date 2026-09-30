use super::*;

fn temporary() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-world-time-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    root
}

#[test]
fn world_time_resumes_saved_phase_and_rejects_corrupt_state() {
    let root = temporary();
    save(&root.join("world.time"), CYCLE_MS * 3 / 4).unwrap();
    let mut clock = Clock::open(&root).unwrap();
    let time = clock.now();
    assert!((CYCLE_MS * 3 / 4..CYCLE_MS * 3 / 4 + 1000).contains(&time));
    clock.finish().unwrap();
    let mut resumed = Clock::open(&root).unwrap();
    assert!(resumed.now() >= time);
    resumed.finish().unwrap();
    fs::write(root.join("world.time"), b"broken").unwrap();
    assert!(Clock::open(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn local_listener_delivers_shared_world_time_admin_changes_and_recovers_it() {
    use crate::protocol::{self, ClientMessage, ServerMessage};
    use std::net::TcpStream;
    let root = temporary();
    drop(crate::world::World::new(7, root.clone()).unwrap());
    save(&root.join("world.time"), CYCLE_MS * 3 / 4).unwrap();
    let connect = |address, profile| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "clock-test".into(),
                profile,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        loop {
            match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::ContentManifestPart {
                    offset,
                    total_len,
                    bytes,
                    ..
                } if offset as usize + bytes.len() == total_len as usize => {
                    protocol::write_client(
                        &mut peer,
                        &ClientMessage::ContentReady {
                            fingerprint: crate::content::catalog().fingerprint(),
                        },
                    )
                    .unwrap();
                }
                ServerMessage::WorldTime { elapsed_ms } => break (peer, elapsed_ms),
                _ => {}
            }
        }
    };
    let (address, server) =
        crate::server::start_local_server_with_admin(7, root.clone(), 0x8675).unwrap();
    let (mut peer, joined) = connect(address, 0x8675);
    assert!(joined >= CYCLE_MS * 3 / 4);
    let (mut second_peer, second_time) = connect(address, 0x8676);
    assert!(second_time >= joined && second_time - joined < 1000);
    let updated = loop {
        if let ServerMessage::WorldTime { elapsed_ms } = protocol::read_server(&mut peer).unwrap() {
            break elapsed_ms;
        }
    };
    assert!(updated > joined && updated - joined < 3000);
    // A remote client cannot promote itself to administrator or move the clock.
    protocol::write_client(
        &mut second_peer,
        &ClientMessage::SetWorldTime {
            elapsed_ms: INITIAL_MS,
        },
    )
    .unwrap();
    loop {
        if let ServerMessage::EditRejected { reason } =
            protocol::read_server(&mut second_peer).unwrap()
        {
            assert!(reason.contains("administrator"));
            break;
        }
    }
    loop {
        if let ServerMessage::WorldTime { elapsed_ms } = protocol::read_server(&mut peer).unwrap() {
            assert!(elapsed_ms >= CYCLE_MS * 3 / 4);
            break;
        }
    }
    protocol::write_client(&mut peer, &ClientMessage::SetWorldTime { elapsed_ms: 0 }).unwrap();
    let changed = loop {
        if let ServerMessage::WorldTime { elapsed_ms } = protocol::read_server(&mut peer).unwrap()
            && elapsed_ms < 3000
        {
            break elapsed_ms;
        }
    };
    loop {
        if let ServerMessage::WorldTime { elapsed_ms } =
            protocol::read_server(&mut second_peer).unwrap()
            && elapsed_ms < 3000
        {
            assert!(elapsed_ms >= changed && elapsed_ms - changed < 1000);
            break;
        }
    }

    server.stop().unwrap();
    drop(peer);
    drop(second_peer);
    let (address, server) = crate::server::start_local_server(7, root.clone()).unwrap();
    let (peer, resumed) = connect(address, 0x8675);
    assert!(resumed >= changed && resumed - changed < 3000);
    server.stop().unwrap();
    drop(peer);
    fs::remove_dir_all(root).unwrap();
}
