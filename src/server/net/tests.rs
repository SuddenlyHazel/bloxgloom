use super::*;
use crate::server::DEFAULT_VIEW;
use crate::server::outbound::OutboundTelemetry;
use crate::server::{INPUT_CAPACITY, run_simulation_ticks};
use std::sync::mpsc::TryRecvError;

#[test]
fn local_server_shutdown_restores_authoritative_position_on_next_start() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-position-loopback-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir(&save).unwrap();
    let profile = 0x7345;
    let (address, server) = super::super::start_local_server(7, save.clone()).unwrap();
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "position-test".into(),
            profile,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    complete_content_handshake(&mut peer);
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Welcome { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::OwnedEntity { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::ActionSession { .. }
    ));
    let original = match protocol::read_server(&mut peer).unwrap() {
        ServerMessage::Position { x, y, z, .. } => [x, y, z],
        other => panic!("expected position, got {other:?}"),
    };
    protocol::write_client(
        &mut peer,
        &ClientMessage::Move {
            seq: 1,
            dx: 0.1,
            dy: 0.0,
            dz: 0.0,
        },
    )
    .unwrap();
    let moved = loop {
        if let ServerMessage::Position {
            ack_seq: 1,
            x,
            y,
            z,
        } = protocol::read_server(&mut peer).unwrap()
        {
            break [x, y, z];
        }
    };
    assert!(moved[0] > original[0]);
    server.stop().unwrap();
    drop(peer);

    let (address, server) = super::super::start_local_server(7, save.clone()).unwrap();
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "position-test".into(),
            profile,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    complete_content_handshake(&mut peer);
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Welcome { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::OwnedEntity { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::ActionSession { .. }
    ));
    match protocol::read_server(&mut peer).unwrap() {
        ServerMessage::Position { x, y, z, .. } => assert_eq!([x, y, z], moved),
        other => panic!("expected restored position, got {other:?}"),
    }
    server.stop().unwrap();
    drop(peer);
    std::fs::remove_dir_all(save).unwrap();
}

#[test]
fn socket_join_reloads_inventory_after_coordinator_refresh() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-join-refresh-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir(&save).unwrap();
    let store = InventoryStore::new(&save).unwrap();
    let profile = 0x1234;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (socket, _) = loop {
        match listener.accept() {
            Ok(accepted) => break accepted,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("accept test socket: {error}"),
        }
    };
    let (input, receiver) = mpsc::sync_channel(8);
    let worker_store = store.clone();
    let outbound = Arc::new(OutboundTelemetry::default());
    let content = ContentHandshake::from_local_catalog().unwrap();
    let worker =
        thread::spawn(move || serve_client(socket, worker_store, input, outbound, content));

    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "test".into(),
            profile,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    complete_content_handshake(&mut peer);
    let first = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let SimulationInput::Join {
        inventory: first_inventory,
        reply: first_reply,
        sender: first_sender,
        socket: first_socket,
        profile: first_profile,
    } = first
    else {
        panic!("expected first join request");
    };
    assert_eq!(first_profile, profile);
    assert_eq!(first_inventory.revision, 0);
    drop(first_sender);
    drop(first_socket);

    let mut updated = first_inventory;
    assert_eq!(
        updated.insert(crate::items::ItemId::new(crate::world::STONE.get()), 1),
        0
    );
    store.save(profile, &updated).unwrap();
    first_reply.send(JoinResponse::RefreshInventory).unwrap();

    let second = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    let SimulationInput::Join {
        inventory: refreshed,
        reply: second_reply,
        sender: second_sender,
        socket: second_socket,
        profile: second_profile,
    } = second
    else {
        panic!("expected refreshed join request");
    };
    assert_eq!(second_profile, profile);
    assert_eq!(refreshed, updated);
    for message in [
        ServerMessage::Welcome { id: 9, seed: 1 },
        ServerMessage::OwnedEntity {
            id: crate::server::entities::EntityId::for_player_session(9)
                .unwrap()
                .get(),
        },
        ServerMessage::ActionSession {
            epoch: 1,
            next_seq: 1,
            acked_seq: 0,
        },
        ServerMessage::Position {
            ack_seq: 0,
            x: 0.0,
            y: 80.0,
            z: 0.0,
        },
        ServerMessage::ViewDistance {
            radius: DEFAULT_VIEW,
        },
        ServerMessage::Inventory {
            revision: refreshed.revision,
            slots: refreshed.slots,
        },
    ] {
        second_sender.try_send(message).unwrap();
    }
    drop(second_sender);
    drop(second_socket);
    second_reply
        .send(JoinResponse::Completed(Box::new(Ok(JoinReply { id: 9 }))))
        .unwrap();

    let connection_id = match protocol::read_server(&mut peer).unwrap() {
        ServerMessage::Welcome { id, .. } => id,
        other => panic!("expected Welcome, got {other:?}"),
    };
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::OwnedEntity { id }
            if id == crate::server::entities::EntityId::for_player_session(connection_id).unwrap().get()
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::ActionSession {
            epoch: 1,
            next_seq: 1,
            acked_seq: 0,
        }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Position { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::ViewDistance { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Inventory { revision: 1, .. }
    ));

    peer.shutdown(Shutdown::Both).unwrap();
    worker.join().unwrap().unwrap();
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
        SimulationInput::Leave { id: 9, .. }
    ));
    std::fs::remove_dir_all(save).unwrap();
}

#[test]
fn join_cleanup_enqueues_one_leave_with_the_next_sequence() {
    let (input, receiver) = mpsc::sync_channel(1);
    let mut cleanup = JoinCleanup {
        id: 17,
        leave_sequence: 42,
        input,
        armed: true,
    };

    cleanup.leave_now();
    drop(cleanup);

    assert!(matches!(
        receiver.try_recv(),
        Ok(SimulationInput::Leave {
            id: 17,
            sequence: 42
        })
    ));
    assert!(matches!(
        receiver.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
}

#[test]
fn connection_admission_never_exceeds_the_client_limit() {
    let mut active = 0;

    for _ in 0..crate::server::DEFAULT_CLIENTS {
        assert!(reactor::has_admission_capacity(active, 0));
        active += 1;
    }

    assert!(!reactor::has_admission_capacity(active, 0));
    assert!(!reactor::has_admission_capacity(
        crate::server::DEFAULT_CLIENTS - 1,
        1
    ));
}

#[test]
fn client_commands_are_rejected_until_content_ready_matches() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-content-ready-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir(&save).unwrap();
    let store = InventoryStore::new(&save).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (input, receiver) = mpsc::sync_channel(8);
    let worker = thread::spawn(move || {
        serve_client(
            socket,
            store,
            input,
            Arc::new(OutboundTelemetry::default()),
            ContentHandshake::from_local_catalog().unwrap(),
        )
    });

    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "not-ready".into(),
            profile: 0x9988,
            content_fingerprint: 0,
        },
    )
    .unwrap();
    let (_fingerprint, manifest) = receive_content_manifest(&mut peer);
    assert!(!manifest.is_empty());
    protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 1 }).unwrap();

    let error = worker.join().unwrap().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(matches!(
        receiver.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
    std::fs::remove_dir_all(save).unwrap();
}

#[test]
fn nonblocking_listener_streams_and_recovers_a_wal_acked_edit() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-live-socket-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir(&save).unwrap();
    let state = super::super::server_state(7, save.clone()).unwrap();
    let store = state.inventory_store.clone();
    let outbound = Arc::clone(&state.outbound);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let (socket, _) = loop {
        match listener.accept() {
            Ok(accepted) => break accepted,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("accept test socket: {error}"),
        }
    };
    let (input, receiver) = mpsc::sync_channel(INPUT_CAPACITY);
    let coordinator = thread::spawn(move || run_simulation_ticks(state, receiver));
    let socket_input = input.clone();
    let content = ContentHandshake::from_local_catalog().unwrap();
    let connection =
        thread::spawn(move || serve_client(socket, store, socket_input, outbound, content));

    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "live-test".into(),
            profile: 0x5678,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    complete_content_handshake(&mut peer);
    let connection_id = match protocol::read_server(&mut peer).unwrap() {
        ServerMessage::Welcome { id, .. } => id,
        other => panic!("expected Welcome, got {other:?}"),
    };
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::OwnedEntity { id }
            if id == crate::server::entities::EntityId::for_player_session(connection_id)
                .unwrap()
                .get()
    ));
    let action_epoch = match protocol::read_server(&mut peer).unwrap() {
        ServerMessage::ActionSession {
            epoch,
            next_seq: 1,
            acked_seq: 0,
        } => epoch,
        other => panic!("expected durable action session, got {other:?}"),
    };
    let position = match protocol::read_server(&mut peer).unwrap() {
        ServerMessage::Position { x, y, z, .. } => [x, y, z],
        other => panic!("expected startup position, got {other:?}"),
    };
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::ViewDistance { .. }
    ));
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Inventory { .. }
    ));

    // An idle reader used to return EAGAIN here on macOS. Keep the socket
    // alive across several server ticks, then prove it still accepts input.
    thread::sleep(Duration::from_millis(100));
    assert!(!connection.is_finished(), "server dropped an idle client");
    protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 2 }).unwrap();
    let mut acknowledged = false;
    let mut streamed_chunk = false;
    for _ in 0..32 {
        match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::ViewDistance { radius: 2 } => acknowledged = true,
            ServerMessage::WorldSnapshotStart(start) => {
                assert_eq!(start.chunk.blocks.len(), crate::world::CHUNK_VOLUME);
                streamed_chunk = true;
            }
            _ => {}
        }
        if acknowledged && streamed_chunk {
            break;
        }
    }
    assert!(acknowledged, "live command was not acknowledged");
    assert!(streamed_chunk, "authoritative terrain was not streamed");

    let block = [
        position[0].floor() as i32,
        position[1].floor() as i32 - 1,
        position[2].floor() as i32,
    ];
    let action_id = u128::from(action_epoch) << 64 | 1;
    protocol::write_client(
        &mut peer,
        &ClientMessage::Edit {
            action_id,
            x: block[0],
            y: block[1],
            z: block[2],
            block: crate::world::AIR,
            slot: 0,
        },
    )
    .unwrap();
    let mut accepted = false;
    for _ in 0..64 {
        if let ServerMessage::ActionResult {
            action_id: received,
            accepted: result,
            reason,
        } = protocol::read_server(&mut peer).unwrap()
            && received == action_id
        {
            assert!(result, "live edit rejected: {reason}");
            accepted = true;
            break;
        }
    }
    assert!(accepted, "live edit received no durable acknowledgement");

    peer.shutdown(Shutdown::Both).unwrap();
    connection.join().unwrap().unwrap();
    drop(input);
    coordinator.join().unwrap().unwrap();
    let mut restarted = super::super::server_state(7, save.clone()).unwrap();
    assert_eq!(
        restarted
            .world
            .get_block(block[0], block[1], block[2])
            .unwrap(),
        crate::world::AIR,
        "WAL-acknowledged edit was lost across restart"
    );
    drop(restarted);
    std::fs::remove_dir_all(save).unwrap();
}

fn complete_content_handshake(peer: &mut TcpStream) {
    let fingerprint = crate::content::catalog().fingerprint();
    let expected_manifest =
        crate::content::ContentManifest::from_catalog(crate::content::catalog())
            .encode()
            .unwrap();
    let (received_fingerprint, manifest) = receive_content_manifest(peer);
    assert_eq!(received_fingerprint, fingerprint);
    assert_eq!(manifest, expected_manifest);
    protocol::write_client(peer, &ClientMessage::ContentReady { fingerprint }).unwrap();
}

#[test]
fn production_reactor_joins_and_commits_an_edit_over_real_tcp() {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-reactor-smoke-{}-{suffix}",
        std::process::id()
    ));
    std::fs::create_dir(&save).unwrap();
    let state = Box::new(crate::server::server_state(7, save.clone()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = mpsc::sync_channel(1);
    let server = thread::Builder::new()
        .name("production-reactor-test".into())
        .spawn(move || reactor::serve_listener_until(listener, state, stop_rx))
        .unwrap();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "reactor-smoke".into(),
                profile: 0xA11CE,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        complete_content_handshake(&mut peer);
        let connection_id = match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::Welcome { id, .. } => id,
            other => panic!("expected Welcome, got {other:?}"),
        };
        assert!(matches!(
            protocol::read_server(&mut peer).unwrap(),
            ServerMessage::OwnedEntity { id }
                if id == crate::server::entities::EntityId::for_player_session(connection_id)
                    .unwrap()
                    .get()
        ));
        let epoch = match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::ActionSession {
                epoch,
                next_seq: 1,
                acked_seq: 0,
            } => epoch,
            other => panic!("expected action session, got {other:?}"),
        };
        let position = match protocol::read_server(&mut peer).unwrap() {
            ServerMessage::Position { x, y, z, .. } => [x, y, z],
            other => panic!("expected position, got {other:?}"),
        };
        assert!(matches!(
            protocol::read_server(&mut peer).unwrap(),
            ServerMessage::ViewDistance { .. }
        ));
        assert!(matches!(
            protocol::read_server(&mut peer).unwrap(),
            ServerMessage::Inventory { .. }
        ));

        let block = [
            position[0].floor() as i32,
            position[1].floor() as i32 - 1,
            position[2].floor() as i32,
        ];
        // Exercise the live snapshot projection -> shared codec -> nonblocking
        // socket path before asking for a durable update to this same epoch.
        let (key, local) = crate::world::world_to_chunk(block[0], block[1], block[2]);
        let mut snapshot = None;
        for _ in 0..128 {
            if let ServerMessage::WorldSnapshotStart(start) =
                protocol::read_server(&mut peer).unwrap()
                && start.chunk.key == key
            {
                snapshot = Some(start);
                break;
            }
        }
        let snapshot = snapshot.expect("production reactor did not stream the edit chunk");
        assert_ne!(snapshot.chunk.block(local), Some(crate::world::AIR));
        let mut pages = Vec::new();
        for index in 0..snapshot.entity_page_count {
            let ServerMessage::EntitySnapshotPage(page) = protocol::read_server(&mut peer).unwrap()
            else {
                panic!("snapshot entity pages must immediately follow their chunk");
            };
            assert_eq!(page.key, key);
            assert_eq!(page.epoch, snapshot.epoch);
            assert_eq!(page.entity_revision, snapshot.entity_revision);
            assert_eq!(page.page_index, index);
            assert_eq!(page.checksum, snapshot.checksum);
            pages.push(page.entities);
        }
        assert_eq!(
            snapshot.checksum,
            protocol::snapshot_checksum(
                &snapshot.chunk,
                snapshot.epoch,
                snapshot.entity_revision,
                &pages,
                crate::content::catalog()
            )
            .unwrap()
        );
        let action_id = u128::from(epoch) << 64 | 1;
        protocol::write_client(
            &mut peer,
            &ClientMessage::Edit {
                action_id,
                x: block[0],
                y: block[1],
                z: block[2],
                block: crate::world::AIR,
                slot: 0,
            },
        )
        .unwrap();
        let mut accepted = false;
        let mut updated = false;
        for _ in 0..512 {
            match protocol::read_server(&mut peer).unwrap() {
                ServerMessage::WorldCommitPart(part) if part.key == key => {
                    assert_eq!(part.epoch, snapshot.epoch);
                    if part.blocks.iter().any(|change| {
                        change.local == local.map(|v| v as u8) && change.block == crate::world::AIR
                    }) {
                        assert_eq!(part.block_from, snapshot.chunk.version);
                        assert!(part.block_to > part.block_from);
                        updated = true;
                    }
                }
                ServerMessage::ActionResult {
                    action_id: received,
                    accepted: result,
                    reason,
                } if received == action_id => {
                    assert!(result, "production reactor rejected edit: {reason}");
                    assert!(
                        updated,
                        "confirmed chunk update must precede its action result"
                    );
                    accepted = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(
            accepted,
            "production reactor did not return a durable result"
        );
        protocol::write_client(
            &mut peer,
            &ClientMessage::ActionAck {
                epoch,
                through_seq: 1,
            },
        )
        .unwrap();
        let _ = peer.shutdown(Shutdown::Both);
        block
    }));

    let _ = stop_tx.send(());
    let server_result = server.join().expect("production server thread panicked");
    let block = match result {
        Ok(block) => block,
        Err(payload) => std::panic::resume_unwind(payload),
    };
    server_result.unwrap();
    let mut restarted = crate::server::server_state(7, save.clone()).unwrap();
    assert_eq!(
        restarted
            .world
            .get_block(block[0], block[1], block[2])
            .unwrap(),
        crate::world::AIR
    );
    drop(restarted);
    std::fs::remove_dir_all(save).unwrap();
}

fn receive_content_manifest(peer: &mut TcpStream) -> (u64, Vec<u8>) {
    let mut manifest = Vec::new();
    let mut total_len = None;
    let mut fingerprint = None;
    loop {
        let ServerMessage::ContentManifestPart {
            fingerprint: part_fingerprint,
            total_len: part_total_len,
            offset,
            bytes,
        } = protocol::read_server(&mut *peer).unwrap()
        else {
            panic!("expected content manifest part");
        };
        assert!(fingerprint.is_none_or(|expected| expected == part_fingerprint));
        fingerprint = Some(part_fingerprint);
        assert_eq!(offset as usize, manifest.len());
        assert!(total_len.is_none_or(|expected| expected == part_total_len as usize));
        total_len = Some(part_total_len as usize);
        manifest.extend(bytes);
        if manifest.len() == total_len.unwrap() {
            break;
        }
    }
    (fingerprint.unwrap(), manifest)
}
