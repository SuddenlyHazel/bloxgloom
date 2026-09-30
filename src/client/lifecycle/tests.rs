use super::*;

pub(crate) fn exercise_player_services(address: &str, path: PathBuf) {
    let mut previous = 0;
    for _ in 0..2 {
        let network = Network::connect(address, 1, 0x7374617465).unwrap();
        let bundle = Arc::downgrade(network.bundle_for_test().unwrap());
        let mut app = ClientApp::new(
            network,
            Config {
                profile: 0x7374617465,
                view_distance: 1,
                ..Default::default()
            },
            path.clone(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match app.network.incoming.recv_timeout(Duration::from_millis(10)) {
                Ok(Incoming::Message(message)) => app.accept(*message),
                Ok(Incoming::Closed(reason)) => panic!("{reason}"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(error) => panic!("{error}"),
            }
            assert!(!app.disconnected, "{:?}", app.failure);
            app.poll_player_services();
            if app.package_ui.as_ref().unwrap().text_at(2) == "Profile progress: level:1" {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "client player panel did not update"
            );
        }
        assert!(app.actions.epoch > previous);
        previous = app.actions.epoch;
        assert_eq!(app.player_states.len(), 1);
        assert_eq!(app.player_states[0].public, b"level:1");
        let stopped = app.network.take_worker_completion().unwrap();
        app.retire_session();
        assert!(
            app.player_services.is_none()
                && app.player_states.is_empty()
                && app.package_ui.is_none()
        );
        drop(app);
        for _ in 0..2 {
            stopped.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        while bundle.upgrade().is_some() {
            assert!(
                Instant::now() < deadline,
                "retired player callback retained session bundle"
            );
            std::thread::yield_now();
        }
    }
}

fn read(app: &ClientApp, deadline: Instant) -> ServerMessage {
    match app
        .network
        .incoming
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("session message deadline")
    {
        Incoming::Message(message) => *message,
        Incoming::Closed(reason) => panic!("unexpected disconnect: {reason}"),
    }
}

fn join(address: &str, path: &std::path::Path) -> ClientApp {
    let attempt =
        super::super::join_worker::Attempt::start(address.to_owned(), path.to_owned(), None)
            .unwrap();
    let prepared = attempt.wait_for_test().unwrap();
    let mut app = ClientApp::new(prepared.network, prepared.config, path.to_owned());
    // A prepared transport can queue messages, but authoritative state has not
    // been consumed. The desktop shell installs GPU/UI resources before polling.
    assert!(app.world_seed.is_none());
    assert!(app.chunks.is_empty());
    assert_eq!(app.actions.epoch, 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.actions.epoch == 0 {
        let message = read(&app, deadline);
        app.accept(message);
        assert!(!app.disconnected);
    }
    assert_eq!(app.actions.next_seq, 1);
    assert!(app.pending_actions.is_empty());
    assert!(app.deferred_actions.is_empty());
    app
}

fn retired(mut app: ClientApp, explicit: bool) {
    let catalog = Arc::downgrade(&app.catalog);
    let stopped = app.network.take_worker_completion().unwrap();
    if explicit {
        app.retire_session();
        assert!(app.package_ui.is_none());
        assert!(app.network.bundle_for_test().is_none());
        assert!(app.network.package_ui().is_none());
        assert!(app.network.package_material().is_none());
        assert!(app.network.package_effect().is_none());
        assert!(app.renderer.is_none());
        assert!(app.pending_commands.is_empty());
        assert!(app.pending_actions.is_empty());
        assert!(app.deferred_actions.is_empty());
        assert_eq!(app.actions.epoch, 0);
        assert!(!app.network.send(ClientMessage::Ping { nonce: 1 }));
    }
    drop(app);
    // Both workers must finish while the real server is still running. No sleeps
    // or server-stop side effect may hide a leaked idle socket reader.
    for _ in 0..2 {
        stopped
            .recv_timeout(Duration::from_secs(10))
            .expect("retired network worker did not stop");
    }
    assert!(catalog.upgrade().is_none(), "old session catalog retained");
}

pub(crate) fn exercise_join_lifecycle(combined: &str, broken: &str, ui: &str, path: PathBuf) {
    Config {
        profile: 0x11fec7,
        view_distance: 1,
        ..Config::default()
    }
    .save(&path)
    .unwrap();
    slow_attempt_can_be_cancelled(combined, &path);
    super::super::join_worker::abandoned_result_retires_transport(combined);
    super::super::joining::tests::failure_and_retry_dispatch(broken, path.clone());
    let mut first = join(combined, &path);
    let bundle = Arc::clone(first.network.bundle_for_test().unwrap());
    assert!(first.catalog.state_by_key("verdant:jade").is_some());
    assert!(first.network.package_material().is_some());
    assert!(first.network.package_effect().is_some());
    let session = first.package_ui.as_mut().unwrap();
    assert_eq!(session.text_at(1), "Jade garden (downloaded mod)");
    session.resize(640, 360, 1.0);
    session.tab(false);
    session.activate(); // A presentation reply must not survive retirement.
    assert!(!bundle.packages()["verdant"].sources.contains_key("plant"));
    let old_id = first.allocate_action_id().unwrap();
    let old_command = ClientMessage::InventoryMove {
        action_id: old_id,
        from: 0,
        to: 1,
        count: 1,
    };
    first.pending_actions.insert(old_id, old_command.clone());
    first.pending_commands.push_back(old_command.clone());
    first.deferred_actions.insert(old_id, Instant::now());
    retired(first, true);

    let mut reconnect = join(combined, &path);
    assert!(
        Arc::ptr_eq(&bundle, reconnect.network.bundle_for_test().unwrap()),
        "exact-byte cache not reused"
    );
    assert_ne!(reconnect.actions.epoch, (old_id >> 64) as u64);
    assert_eq!(reconnect.package_ui.as_ref().unwrap().feedback(), None);
    assert!(reconnect.network.send(old_command));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let message = read(&reconnect, deadline);
        if let ServerMessage::ActionResult {
            action_id,
            accepted,
            reason,
        } = &message
            && *action_id == old_id
        {
            assert!(!accepted, "old epoch accepted");
            assert!(reason.contains("session"), "{reason}");
            // Production dispatch must reject stale results before touching UI.
            reconnect.accept(message);
            assert!(
                reconnect
                    .failure
                    .as_deref()
                    .unwrap()
                    .contains("current action session")
            );
            break;
        }
        reconnect.accept(message);
    }
    retired(reconnect, true);

    for _ in 0..2 {
        let attempt =
            super::super::join_worker::Attempt::start(broken.to_owned(), path.clone(), None)
                .unwrap();
        let error = attempt.wait_for_test().err().expect("startup must fail");
        let message = error.to_string();
        assert!(message.contains(broken), "{message}");
        assert!(message.contains("package client startup"), "{message}");
        assert!(message.contains("uidemo@1.0.0:client_startup"), "{message}");
        assert!(message.contains("deliberate join failure"), "{message}");
    }
    // A failed startup may leave verified bytes in the bounded cache, never
    // partial session registrations. A healthy retry constructs all state anew.
    retired(join(combined, &path), false);
    let ui_app = join(ui, &path);
    assert!(ui_app.catalog.state_by_key("verdant:jade").is_none());
    assert!(ui_app.network.package_material().is_none());
    assert!(ui_app.network.package_effect().is_none());
    assert_eq!(
        ui_app.package_ui.as_ref().unwrap().text_at(1),
        "different server"
    );
    let ui_bundle = Arc::clone(ui_app.network.bundle_for_test().unwrap());
    assert!(!ui_bundle.packages().contains_key("verdant"));
    retired(ui_app, false);
    let mut ui_retry = join(ui, &path);
    assert!(Arc::ptr_eq(
        &ui_bundle,
        ui_retry.network.bundle_for_test().unwrap()
    ));
    assert_eq!(
        ui_retry.package_ui.as_ref().unwrap().text_at(1),
        "different server"
    );
    // Deterministic local encoding failure: no invalid bytes reach the server.
    // The writer must wake the otherwise idle reader and report closure.
    assert!(ui_retry.network.send(ClientMessage::Move {
        seq: 1,
        dx: f32::NAN,
        dy: 0.0,
        dz: 0.0,
    }));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match ui_retry
            .network
            .incoming
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("writer failure did not close reader")
        {
            Incoming::Message(message) => ui_retry.accept(*message),
            Incoming::Closed(reason) => {
                ui_retry.fail_session(format!("disconnected: {reason}"));
                break;
            }
        }
    }
    retired(ui_retry, true);
    let back = join(combined, &path);
    assert!(back.network.package_material().is_some());
    assert!(back.network.package_effect().is_some());
    assert_eq!(
        back.package_ui.as_ref().unwrap().text_at(1),
        "Jade garden (downloaded mod)"
    );
    assert_eq!(
        back.network.bundle_for_test().unwrap().bytes(),
        bundle.bytes()
    );
    retired(back, true);
}

fn slow_attempt_can_be_cancelled(address: &str, path: &std::path::Path) {
    use super::super::join_worker::Attempt;
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;

    let upstream_address = address.to_owned();
    let relay = TcpListener::bind("127.0.0.1:0").unwrap();
    let relay_address = relay.local_addr().unwrap();
    let (held, holding) = mpsc::sync_channel(1);
    let relay_worker = std::thread::spawn(move || {
        let (mut downstream, _) = relay.accept().unwrap();
        downstream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut upstream = TcpStream::connect(upstream_address).unwrap();
        upstream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let hello = crate::protocol::read_client(&mut downstream).unwrap();
        crate::protocol::write_client(&mut upstream, &hello).unwrap();
        // Gate a response from the real production nonblocking listener, rather
        // than using a fake server as evidence that startup is responsive.
        let first = crate::protocol::read_server(&mut upstream).unwrap();
        assert!(matches!(first, ServerMessage::BundleOffer { .. }));
        held.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(
            downstream.read(&mut byte).unwrap(),
            0,
            "cancelled handshake socket retained"
        );
    });
    let mut attempt = Attempt::start(relay_address.to_string(), path.to_owned(), None).unwrap();
    holding.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(attempt.poll().is_none(), "slow join must remain pending");
    assert_eq!(attempt.control.snapshot().0, "initial handshake");
    attempt.cancel();
    let error = attempt
        .wait_for_test()
        .err()
        .expect("cancelled attempt became live");
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    relay_worker.join().unwrap();
}

#[test]
fn preparation_failure_is_preserved_and_retires_session() {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        PathBuf::new(),
    );
    app.fail_session("package material GPU preparation: test failure");
    app.fail_session("connection closed");
    assert_eq!(
        app.failure.as_deref(),
        Some("package material GPU preparation: test failure")
    );
    assert!(app.disconnected);
    assert!(app.renderer.is_none());
}

#[test]
fn player_roster_accepts_newer_snapshots_and_is_cleared_when_session_retires() {
    let mut app = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join("unused-player-roster-config"),
    );
    let alice = crate::protocol::PlayerSummary {
        profile: u128::MAX,
        session: u64::MAX,
        name: "Alice".into(),
    };
    app.accept(ServerMessage::PlayerRoster {
        revision: 2,
        players: vec![alice.clone()],
    });
    app.accept(ServerMessage::PlayerRoster {
        revision: 1,
        players: vec![],
    });
    assert_eq!(app.player_roster, vec![alice]);
    app.retire_session();
    assert!(app.player_roster.is_empty());
    assert_eq!(app.roster_revision, 0);
    app.accept(ServerMessage::PlayerRoster {
        revision: 3,
        players: vec![crate::protocol::PlayerSummary {
            profile: 2,
            session: 1,
            name: "late".into(),
        }],
    });
    assert!(
        app.player_roster.is_empty(),
        "late roster resurrected a retired session"
    );
}

#[test]
fn public_player_snapshots_validate_session_and_services_then_clear_on_retirement() {
    let mut network = Network::disconnected_for_test();
    network.profile = 1;
    let mut catalog = (*network.catalog).clone();
    catalog
        .client_runtime_identity(crate::content::client_metadata::Identity::new(
            b'Q',
            "demo:progress".into(),
            b"contract",
        ))
        .unwrap();
    network.catalog = Arc::new(catalog);
    let mut app = ClientApp::new(
        network,
        Config {
            profile: 1,
            ..Default::default()
        },
        std::env::temp_dir().join("unused-player-state-config"),
    );
    app.accept(ServerMessage::ActionSession {
        epoch: 5,
        next_seq: 1,
        acked_seq: 0,
    });
    let state = crate::protocol::PlayerState {
        key: "demo:progress".into(),
        revision: 1,
        public: vec![0, 255],
    };
    app.accept(ServerMessage::PlayerStates {
        profile: 1,
        session: 5,
        snapshot: 2,
        states: vec![state.clone()],
    });
    assert_eq!(app.player_states, vec![state.clone()]);
    app.accept(ServerMessage::PlayerStates {
        profile: 1,
        session: 5,
        snapshot: 1,
        states: vec![],
    });
    assert_eq!(app.player_states, vec![state.clone()]);
    app.retire_session();
    app.accept(ServerMessage::PlayerStates {
        profile: 1,
        session: 5,
        snapshot: 3,
        states: vec![state.clone()],
    });
    assert!(app.player_states.is_empty());
    assert_eq!(app.player_state_snapshot, 0);
    for (profile, session, states) in [
        (2, 5, vec![state.clone()]),
        (1, 4, vec![state]),
        (1, 5, vec![]),
    ] {
        let mut network = Network::disconnected_for_test();
        network.profile = 1;
        network.catalog = Arc::clone(&app.catalog);
        let mut fresh = ClientApp::new(
            network,
            Config {
                profile: 1,
                ..Default::default()
            },
            std::env::temp_dir().join("unused-fresh-player-state-config"),
        );
        fresh.accept(ServerMessage::ActionSession {
            epoch: 5,
            next_seq: 1,
            acked_seq: 0,
        });
        fresh.accept(ServerMessage::PlayerStates {
            profile,
            session,
            snapshot: 1,
            states,
        });
        assert!(fresh.disconnected && fresh.player_states.is_empty());
    }
}

#[test]
fn player_notices_present_only_current_session_and_kicks_retire_it() {
    for (profile, session, kicked) in [(1, 5, false), (1, 5, true), (2, 5, false), (1, 4, false)] {
        let mut network = Network::disconnected_for_test();
        network.profile = 1;
        let mut app = ClientApp::new(
            network,
            Config::default(),
            std::env::temp_dir().join("unused-notice-config"),
        );
        app.accept(ServerMessage::ActionSession {
            epoch: 5,
            next_seq: 1,
            acked_seq: 0,
        });
        app.accept(ServerMessage::PlayerNotice {
            profile,
            session,
            kicked,
            text: "Session notice".into(),
        });
        if (profile, session, kicked) == (1, 5, false) {
            assert!(!app.disconnected);
            assert_eq!(app.status.as_ref().unwrap().0, "Session notice");
        } else {
            assert!(app.disconnected);
            assert_eq!(app.actions.epoch, 0);
            let reason = if kicked {
                "Removed by server: Session notice"
            } else {
                "Player notice has wrong session identity"
            };
            assert_eq!(app.failure.as_deref(), Some(reason));
            app.accept(ServerMessage::PlayerNotice {
                profile: 1,
                session: 5,
                kicked: false,
                text: "Late notice".into(),
            });
            assert!(app.status.is_none());
        }
    }
}
