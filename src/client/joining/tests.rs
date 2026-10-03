use super::*;

#[test]
fn package_reload_notice_retires_session_and_starts_same_address_join() {
    let mut shell = JoinApp::new("127.0.0.1:0", true);
    shell.config_path = std::env::temp_dir().join(format!(
        "bloxgloom-reload-client-{}.cfg",
        std::process::id()
    ));
    let mut live = ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        shell.config_path.clone(),
    );
    live.accept(ServerMessage::PackageReload {
        reconnect: false,
        text: "reload rejected".into(),
    });
    shell.live = Some(live);
    assert!(!shell.reload_if_requested());
    assert!(shell.live.is_some());
    shell
        .live
        .as_mut()
        .unwrap()
        .accept(ServerMessage::PackageReload {
            reconnect: true,
            text: "reload complete".into(),
        });
    assert!(shell.reload_if_requested());
    assert!(shell.live.is_none());
    assert!(shell.attempt.is_some());
    assert_eq!(shell.address, "127.0.0.1:0");
    shell.finish();
    let _ = std::fs::remove_file(shell.config_path);
}

pub(in crate::client) fn failure_and_retry_dispatch(address: &str, path: PathBuf) {
    let mut app = JoinApp::new(address, false);
    app.config_path = path;
    for _ in 0..2 {
        app.action(); // Same action as the visible Join button.
        assert!(app.error.is_none());
        assert!(app.live.is_none());
        app.attempt.as_mut().unwrap().wait_finished_for_test();
        // Calling start while a result is pending must not replace the retained
        // attempt (or its package-attributed error) with another worker.
        app.start();
        app.poll();
        assert!(app.attempt.is_none());
        assert!(app.live.is_none());
        let error = app.error.as_deref().unwrap();
        assert!(error.contains("package client startup"), "{error}");
        assert!(error.contains("uidemo@1.0.0:client_startup"), "{error}");
        assert!(error.contains("deliberate join failure"), "{error}");
    }
}
