use super::*;
use crate::server::{DEFAULT_VIEW, outbound::OutboundTelemetry};
use std::sync::mpsc::TryRecvError;

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
    let worker = thread::spawn(move || serve_client(socket, worker_store, input));

    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "test".into(),
            profile,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
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
    assert_eq!(updated.insert(crate::world::STONE, 1), 0);
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
    let telemetry = Arc::new(OutboundTelemetry::default());
    for message in [
        ServerMessage::Welcome { id: 9, seed: 1 },
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
        assert!(telemetry.try_send(&second_sender, message));
    }
    drop(second_sender);
    drop(second_socket);
    second_reply
        .send(JoinResponse::Completed(Box::new(Ok(JoinReply { id: 9 }))))
        .unwrap();

    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Welcome { id: 9, seed: 1 }
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
    let active = AtomicUsize::new(0);

    for _ in 0..MAX_CLIENTS {
        assert!(reserve_connection(&active));
    }

    assert!(!reserve_connection(&active));
    assert_eq!(active.load(Ordering::Acquire), MAX_CLIENTS);
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
    let connection = thread::spawn(move || serve_client(socket, store, socket_input));

    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "live-test".into(),
            profile: 0x5678,
            content_fingerprint: crate::content::catalog().fingerprint(),
        },
    )
    .unwrap();
    assert!(matches!(
        protocol::read_server(&mut peer).unwrap(),
        ServerMessage::Welcome { .. }
    ));
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
            ServerMessage::Chunk(_) => streamed_chunk = true,
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
    let action_id = 0x5678u128;
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
