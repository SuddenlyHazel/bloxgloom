use super::*;

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
