//! Real nonblocking listener, asynchronous chunk residency, and WAL recovery.
use super::*;
use crate::protocol::{self, ClientMessage, ServerMessage};
use std::net::{TcpListener, TcpStream};

#[test]
fn durable_intent_chain_runs_through_real_listener_and_recovers_once() {
    listener_chain(false);
}

#[test]
fn durable_intent_bootstrap_chain_runs_through_real_listener_and_recovers_once() {
    listener_chain(true);
}

fn listener_chain(bootstrap: bool) {
    let path = save();
    let (observed, completed) = std::sync::mpsc::channel();
    let startup = ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&Ignitions {
            fanout: 1,
            bootstrap,
            observed: Some(observed),
        })
        .unwrap();
    let state = Box::new(server_state_with_startup(7, path.clone(), 2, startup).unwrap());
    if bootstrap {
        for x in [9, 10] {
            assert!(
                state
                    .system_runtime
                    .owner_snapshot(&system_id(), owner(x))
                    .is_none()
            );
        }
    }
    let catalog = state.world.catalog_arc();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        crate::server::net::serve_listener_with_stats(
            listener,
            state,
            stopped,
            Arc::new(crate::server::net::TransportStats::default()),
        )
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "intent-proof".into(),
                profile: 0x1122,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        loop {
            let ServerMessage::ContentManifestPart {
                fingerprint,
                total_len,
                offset,
                bytes,
            } = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap()
            else {
                panic!("expected manifest");
            };
            assert_eq!(fingerprint, catalog.fingerprint());
            if offset as usize + bytes.len() == total_len as usize {
                break;
            }
        }
        protocol::write_client(
            &mut peer,
            &ClientMessage::ContentReady {
                fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            assert!(std::time::Instant::now() < deadline, "join did not finish");
            if matches!(
                protocol::read_server_with_catalog(&mut peer, &catalog).unwrap(),
                ServerMessage::ActionSession { .. }
            ) {
                break;
            }
        }
        // Exact callback completion signal, not a sleep or speculative retry
        // loop. A later movement ACK crosses the callback's receipt barrier.
        completed.recv_timeout(Duration::from_secs(10)).unwrap();
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
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                std::time::Instant::now() < deadline,
                "movement barrier did not finish"
            );
            if matches!(
                protocol::read_server_with_catalog(&mut peer, &catalog).unwrap(),
                ServerMessage::Position { ack_seq: 1, .. }
            ) {
                break;
            }
        }
    }));
    stop.send(()).unwrap();
    server.join().unwrap().unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    let mut state =
        server_state_with_startup(7, path.clone(), 2, startup_with_bootstrap(1, bootstrap))
            .unwrap();
    assert_eq!(
        (value(&state, 8), value(&state, 9), value(&state, 10)),
        (1, 1, 1)
    );
    assert!(pending(&state, 9).is_empty());
    assert!(pending(&state, 10).is_empty());
    for x in [9, 10] {
        let [x, y, z] = cell(x);
        assert_eq!(state.world.get_block(x, y, z).unwrap(), GLOWSTONE);
    }
    drop(state);
    std::fs::remove_dir_all(path).unwrap();
}
