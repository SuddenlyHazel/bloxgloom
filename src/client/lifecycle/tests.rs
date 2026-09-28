use super::*;

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
    let network = Network::connect(address, 1, 0x11fec7).unwrap();
    let mut app = ClientApp::new(network, Config::default(), path.to_owned());
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
        let error = Network::connect(broken, 1, 0x11fec7)
            .err()
            .expect("startup must fail");
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
