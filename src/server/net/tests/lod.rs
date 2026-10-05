//! Real reactor, nonblocking listener, and isolated authoritative save.
use super::*;
#[path = "lod/loading.rs"]
mod loading;
#[test]
fn distant_tile_streams_with_session_and_keeps_gameplay_ping_responsive() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lod-loopback-{}-{stamp}",
        std::process::id()
    ));
    let (address, server) = crate::server::start_local_server(7, save.clone()).unwrap();
    let result = std::panic::catch_unwind(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "lod-test".into(),
                profile: 0x1_0d,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        complete_content_handshake(&mut peer);
        let mut session = 0;
        while session == 0 {
            if let ServerMessage::ActionSession { epoch, .. } =
                protocol::read_server(&mut peer).unwrap()
            {
                session = epoch;
            }
        }
        let position = loop {
            if let ServerMessage::Position { x, y, z, .. } =
                protocol::read_server(&mut peer).unwrap()
            {
                break [x, y, z];
            }
        };
        protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 1 }).unwrap();
        protocol::write_client(&mut peer, &ClientMessage::LodConfig { horizon: 4096 }).unwrap();
        let key =
            crate::lod::TileKey::containing(0, position[0] as i32, position[2] as i32).unwrap();
        protocol::write_client(&mut peer, &ClientMessage::LodRequest { request: 3, key }).unwrap();
        protocol::write_client(&mut peer, &ClientMessage::Ping { nonce: 0x10d }).unwrap();
        let mut got_tile = false;
        let mut got_ping = false;
        let mut got_status = false;
        let deadline = Instant::now() + Duration::from_secs(15);
        while !(got_tile && got_ping && got_status) {
            assert!(Instant::now() < deadline, "LOD startup timed out");
            match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::LodStatus {
                    session: received,
                    horizon,
                    max_level,
                } => {
                    assert_eq!(received, session);
                    assert_eq!(horizon, 1024);
                    assert_eq!(max_level, 4);
                    got_status = true;
                }
                ServerMessage::LodTile {
                    session: received,
                    request: 3,
                    tile,
                } => {
                    assert_eq!(received, session);
                    assert_eq!(tile.key, key);
                    tile.validate(crate::content::catalog()).unwrap();
                    got_tile = true;
                }
                ServerMessage::LodUnavailable { request: 3, .. } => {
                    panic!("base terrain unavailable")
                }
                ServerMessage::Pong { nonce: 0x10d } => got_ping = true,
                _ => {}
            }
        }
        protocol::write_client(
            &mut peer,
            &ClientMessage::LodRequest {
                request: 4,
                key: crate::lod::TileKey {
                    level: 20,
                    x: 0,
                    z: 0,
                },
            },
        )
        .unwrap();
        loop {
            if let ServerMessage::LodUnavailable {
                session: received,
                request: 4,
                ..
            } = protocol::read_server(&mut peer).unwrap()
            {
                assert_eq!(received, session);
                break;
            }
        }
    });
    server.stop().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

#[test]
fn committed_edit_refreshes_two_distant_clients_without_waiting_for_checkpoint() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lod-edit-loopback-{}-{stamp}",
        std::process::id()
    ));
    let mut state = Box::new(crate::server::server_state(7, save.clone()).unwrap());
    for x in -1..=0 {
        for z in -1..=0 {
            state
                .world
                .get_chunk(crate::world::ChunkKey { x, y: 5, z })
                .unwrap();
        }
    }
    let mut edits = Vec::new();
    for x in -1..=5 {
        for z in -1..=2 {
            edits.push((x, 94, z, crate::world::STONE));
        }
    }
    edits.push((3, 95, 0, crate::world::GLOWSTONE));
    let prepared = state.world.prepare_edits(&edits).unwrap();
    state.world.apply_prepared_edits(prepared).unwrap();
    for profile in [0x10d1, 0x10d2] {
        state.position_store.save(profile, [0.5, 95., 0.5]).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = mpsc::sync_channel(1);
    let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stopped));
    let result = std::panic::catch_unwind(|| {
        let connect = |profile| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: format!("lod-{profile}"),
                    profile,
                    content_fingerprint: crate::content::catalog().fingerprint(),
                },
            )
            .unwrap();
            complete_content_handshake(&mut peer);
            let epoch = loop {
                if let ServerMessage::ActionSession { epoch, .. } =
                    protocol::read_server(&mut peer).unwrap()
                {
                    break epoch;
                }
            };
            protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 1 }).unwrap();
            protocol::write_client(&mut peer, &ClientMessage::LodConfig { horizon: 512 }).unwrap();
            (peer, epoch)
        };
        let (mut first, epoch) = connect(0x10d1);
        let (mut second, _) = connect(0x10d2);
        let key = crate::lod::TileKey {
            level: 1,
            x: 0,
            z: 0,
        };
        let receive_tile = |peer: &mut TcpStream, request| -> crate::lod::LodTile {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                assert!(Instant::now() < deadline, "LOD edit tile timeout");
                match protocol::read_server(&mut *peer).unwrap() {
                    ServerMessage::LodTile {
                        request: got, tile, ..
                    } if got == request => break tile,
                    ServerMessage::LodUnavailable { request: got, .. } if got == request => {
                        panic!("edited tile unavailable")
                    }
                    _ => {}
                }
            }
        };
        let mut gpu_qa = crate::render::lod::qa::EditGpu::from_env();
        for (client, peer) in [&mut first, &mut second].into_iter().enumerate() {
            protocol::write_client(&mut *peer, &ClientMessage::LodRequest { request: 1, key })
                .unwrap();
            let tile = receive_tile(peer, 1);
            assert!(
                tile.columns[1]
                    .spans
                    .iter()
                    .any(|s| s.bottom <= 95 && s.top > 95 && s.state == crate::world::GLOWSTONE)
            );
            if client == 0
                && let Some(qa) = &mut gpu_qa
            {
                qa.warm(&tile);
            }
        }
        let started = Instant::now();
        protocol::write_client(
            &mut first,
            &ClientMessage::Edit {
                action_id: (u128::from(epoch) << 64) | 1,
                x: 3,
                y: 95,
                z: 0,
                block: crate::world::AIR,
                slot: 0,
            },
        )
        .unwrap();
        for (client, peer) in [&mut first, &mut second].into_iter().enumerate() {
            let revision = loop {
                match protocol::read_server(&mut *peer).unwrap() {
                    ServerMessage::LodInvalidate {
                        key: got, revision, ..
                    } if got == key => break revision,
                    ServerMessage::ActionResult {
                        accepted: false,
                        reason,
                        ..
                    } => panic!("edit rejected: {reason}"),
                    _ => {}
                }
            };
            protocol::write_client(&mut *peer, &ClientMessage::LodRequest { request: 2, key })
                .unwrap();
            let tile = receive_tile(peer, 2);
            assert!(tile.revision >= revision);
            assert!(
                !tile.columns[1]
                    .spans
                    .iter()
                    .any(|s| s.bottom <= 95 && s.top > 95 && s.state == crate::world::GLOWSTONE)
            );
            if client == 0
                && let Some(qa) = &mut gpu_qa
            {
                qa.updated(&tile, started);
            }
        }
        eprintln!(
            "LOD committed edit to two refreshed clients: {}ms",
            started.elapsed().as_millis()
        );
    });
    stop.send(()).unwrap();
    server.join().unwrap().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

struct DistantMarker;
impl bloxgloom_host_api::generation::Contributor for DistantMarker {
    fn generate(
        &self,
        context: bloxgloom_host_api::generation::Context,
        output: &mut bloxgloom_host_api::generation::Output,
    ) -> Result<(), bloxgloom_host_api::generation::GenerationError> {
        if context.chunk[1] == 4 {
            output.set([0, 12, 0], "bloxgloom:glowstone")?;
        }
        Ok(())
    }
}
impl bloxgloom_host_api::Extension for DistantMarker {
    fn register(
        &self,
        host: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        host.generation_contributor(bloxgloom_host_api::generation::Registration {
            key: "lod:marker".into(),
            revision: 1,
            contributor: Arc::new(DistantMarker),
        })
    }
}
#[test]
fn registered_contributor_summary_negotiates_bounded_fallback_and_preserves_marker() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lod-contributor-{}-{stamp}",
        std::process::id()
    ));
    let startup =
        crate::server::startup::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
            .with_extension(&DistantMarker)
            .unwrap();
    let state =
        Box::new(crate::server::server_state_with_startup(7, save.clone(), 8, startup).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = mpsc::sync_channel(1);
    let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stopped));
    let result = std::panic::catch_unwind(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "lod-contributor".into(),
                profile: 0x10dc,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        complete_content_handshake(&mut peer);
        loop {
            if matches!(
                protocol::read_server(&mut peer).unwrap(),
                ServerMessage::ActionSession { .. }
            ) {
                break;
            }
        }
        protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 1 }).unwrap();
        protocol::write_client(&mut peer, &ClientMessage::LodConfig { horizon: 1024 }).unwrap();
        loop {
            if let ServerMessage::LodStatus {
                max_level, horizon, ..
            } = protocol::read_server(&mut peer).unwrap()
            {
                assert_eq!(max_level, 3);
                assert_eq!(horizon, 512);
                break;
            }
        }
        let key = crate::lod::TileKey {
            level: 2,
            x: 0,
            z: 0,
        };
        let started = Instant::now();
        protocol::write_client(&mut peer, &ClientMessage::LodRequest { request: 1, key }).unwrap();
        let tile = loop {
            match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::LodTile {
                    request: 1, tile, ..
                } => break tile,
                ServerMessage::LodUnavailable { request: 1, .. } => {
                    panic!("contributor fallback unavailable")
                }
                _ => {}
            }
        };
        assert!(
            tile.columns[0]
                .spans
                .iter()
                .any(|s| s.bottom <= 76 && s.top > 76 && s.state == crate::world::GLOWSTONE)
        );
        eprintln!(
            "LOD exact contributor128-block tile cold: {}ms",
            started.elapsed().as_millis()
        );
        // Resident near chunks can extend the builtin band, but heights never
        // observed by either generation or streaming remain explicitly unknown.
        assert!(!tile.columns[0].known(300, 316));
    });
    stop.send(()).unwrap();
    server.join().unwrap().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
