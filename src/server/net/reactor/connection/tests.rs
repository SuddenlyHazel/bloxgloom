use super::*;
use crate::protocol::{self, ClientMessage};
use crate::server::outbound::OutboundTelemetry;
use std::net::{SocketAddr, TcpListener};
use std::thread;
use std::time::Duration;

fn connected_streams() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    server.set_nonblocking(true).unwrap();
    (server, peer)
}

fn test_connection() -> (Connection, TcpStream, Arc<ContentHandshake>) {
    let (socket, peer) = connected_streams();
    let outbound = Arc::new(OutboundTelemetry::default());
    let (sender, receiver) = outbound.client_queue();
    let stats = Arc::new(TransportStats::default());
    let mut connection = Connection::new(socket, outbound, 1, Arc::clone(&stats));
    connection.outbound_sender = Some(sender);
    connection.outbound_receiver = Some(receiver);
    (
        connection,
        peer,
        ContentHandshake::from_local_catalog().unwrap(),
    )
}

#[test]
fn frame_reader_keeps_partial_prefixes_until_the_payload_is_complete() {
    let (mut connection, _peer, _content) = test_connection();
    let mut frame = Vec::new();
    protocol::write_client(&mut frame, &ClientMessage::SetView { radius: 3 }).unwrap();

    connection.input_buffer.extend_from_slice(&frame[..2]);
    assert_eq!(connection.take_complete_frame().unwrap(), None);
    connection.input_buffer.extend_from_slice(&frame[2..6]);
    assert_eq!(connection.take_complete_frame().unwrap(), None);
    connection.input_buffer.extend_from_slice(&frame[6..]);
    assert_eq!(connection.take_complete_frame().unwrap(), Some(frame));
    assert!(connection.input_buffer.is_empty());
}

#[test]
fn commands_received_after_content_ready_wait_for_join_completion() {
    let (mut connection, _peer, content) = test_connection();
    let codecs = CodecWorkers::new(
        Arc::clone(&content.catalog),
        2,
        Arc::clone(&connection.stats),
        Arc::new(Poller::new().unwrap()),
    )
    .unwrap();
    let mut command = Vec::new();
    protocol::write_client(&mut command, &ClientMessage::SetView { radius: 4 }).unwrap();
    connection.input_buffer = command.clone();
    connection.phase = Phase::LoadingInventory;

    let (input, receiver) = mpsc::sync_channel(2);
    assert!(!connection.poll_read(&codecs).unwrap());
    assert_eq!(connection.input_buffer, command);
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

    connection.phase = Phase::Active;
    connection.player_id = Some(77);
    connection.poll_read(&codecs).unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if connection
            .poll_decode(Instant::now(), &content, &input)
            .unwrap()
        {
            break;
        }
        assert!(Instant::now() < deadline, "codec worker did not finish");
        thread::yield_now();
    }
    assert!(matches!(
        receiver.try_recv(),
        Ok(SimulationInput::Command {
            id: 77,
            sequence: 1,
            message: ClientMessage::SetView { radius: 4 }
        })
    ));
}

#[test]
fn full_coordinator_queue_preserves_one_command_and_its_sequence() {
    let (mut connection, _peer, content) = test_connection();
    connection.phase = Phase::Active;
    connection.player_id = Some(77);
    let (input, receiver) = mpsc::sync_channel(1);
    input
        .try_send(SimulationInput::Leave {
            id: 99,
            sequence: 1,
        })
        .unwrap();

    connection
        .handle_message(
            Instant::now(),
            ClientMessage::SetView { radius: 2 },
            &content,
            &input,
        )
        .unwrap();
    assert_eq!(connection.sequence, 0);
    assert_eq!(
        connection.pending_command,
        Some(ClientMessage::SetView { radius: 2 })
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(SimulationInput::Leave { id: 99, .. })
    ));

    connection.flush_pending_command(&input).unwrap();
    assert_eq!(connection.sequence, 1);
    assert!(connection.pending_command.is_none());
    assert!(matches!(
        receiver.try_recv(),
        Ok(SimulationInput::Command {
            id: 77,
            sequence: 1,
            message: ClientMessage::SetView { radius: 2 }
        })
    ));
}

#[test]
fn bundle_frames_are_shared_and_stalled_transfers_keep_an_absolute_deadline() {
    use crate::server::client_bundle::{CacheKey, ClientBundle};
    use sha2::{Digest, Sha256};
    let bytes = b"BGCLIENT\x06\0\0\0\0\0\0\0\0";
    let bundle =
        ClientBundle::decode_verify(bytes, CacheKey::from_bytes(Sha256::digest(bytes).into()))
            .unwrap();
    let (mut first, _peer, local) = test_connection();
    let (mut second, _peer2, _) = test_connection();
    let content = ContentHandshake::with_bundle(Arc::clone(&local.catalog), Some(&bundle)).unwrap();
    let codecs = CodecWorkers::new(
        Arc::clone(&content.catalog),
        2,
        Arc::clone(&first.stats),
        Arc::new(Poller::new().unwrap()),
    )
    .unwrap();
    let (input, receiver) = mpsc::sync_channel(1);
    let now = Instant::now();
    for connection in [&mut first, &mut second] {
        connection
            .handle_message(
                now,
                ClientMessage::Hello {
                    name: "bundle".into(),
                    profile: 1,
                    content_fingerprint: 0,
                },
                &content,
                &input,
            )
            .unwrap();
        let deadline = connection.deadline;
        connection.prepare_write(&content, &codecs).unwrap();
        assert!(Arc::ptr_eq(
            &connection.pending_write.as_ref().unwrap().bytes,
            &content.bundle.as_ref().unwrap().offer
        ));
        connection.poll_write().unwrap();
        let identity = content.bundle.as_ref().unwrap().identity;
        connection
            .handle_message(
                now,
                ClientMessage::BundleRequest { identity },
                &content,
                &input,
            )
            .unwrap();
        connection.prepare_write(&content, &codecs).unwrap();
        assert!(Arc::ptr_eq(
            &connection.pending_write.as_ref().unwrap().bytes,
            &content.bundle.as_ref().unwrap().parts[0]
        ));
        assert_eq!(connection.bundle_index, 0);
        assert_eq!(connection.deadline, deadline);
        assert!(connection.inventory_receiver.is_none());
        assert!(connection.pending_encode_frame.is_none());
        assert!(connection.encode_receiver.is_none());
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
        // Advance deterministic host time, not the socket or a sleeping worker.
        let workers = InventoryWorkers {
            sender: None,
            workers: Vec::new(),
        };
        assert_eq!(
            connection
                .poll(
                    deadline,
                    &content,
                    &workers,
                    &codecs,
                    &input,
                    &mut VecDeque::new(),
                    Readiness::default()
                )
                .unwrap_err()
                .kind(),
            ErrorKind::TimedOut
        );
    }
    assert!(Arc::ptr_eq(
        &first.pending_write.as_ref().unwrap().bytes,
        &second.pending_write.as_ref().unwrap().bytes
    ));
}
