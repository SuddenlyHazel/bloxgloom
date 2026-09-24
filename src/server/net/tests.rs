use super::*;
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
    let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (socket, _) = listener.accept().unwrap();
    let (input, receiver) = mpsc::sync_channel(8);
    let worker_store = store.clone();
    let worker = thread::spawn(move || {
        serve_client(
            socket,
            worker_store,
            input,
            Arc::new(OutboundTelemetry::default()),
        )
    });

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
    drop(second_sender);
    drop(second_socket);
    second_reply
        .send(JoinResponse::Completed(Box::new(Ok(JoinReply {
            id: 9,
            seed: 1,
            position: [0.0, 80.0, 0.0],
            inventory: refreshed,
        }))))
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
