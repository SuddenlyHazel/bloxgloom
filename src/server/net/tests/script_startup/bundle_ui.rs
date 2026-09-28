//! Real nonblocking listener -> client install/cache -> local UI session.
use super::*;

#[test]
fn package_ui_worker_dispatches_on_join_and_resets_on_cached_reconnect() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/packages"),
        )
        .unwrap();
    let state =
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap();
    let key = state.client_bundle.as_ref().unwrap().cache_key();
    let fingerprint = state.world.catalog_arc().fingerprint();
    gameplay::serve(Box::new(state), |address| {
        for profile in [0x911, 0x912] {
            crate::client::connect_ui_probe(&address.to_string(), profile, |bundle, session| {
                assert_eq!(bundle.cache_key(), key);
                session.resize(640, 360, 1.0);
                assert!(session.focused_id().is_none());
                assert_eq!(session.text_at(1), "Welcome to the garden");
                assert_eq!(session.text_at(4), "Moss & stone");
                session.tab(false);
                assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
                assert_eq!(session.event(), Some("uidemo:name-changed"));
                session.edit(false, Some(" local edit"));
                session.wait_for_presentation().unwrap();
                assert_eq!(session.text_at(1), "Garden: Moss & stone local edit");
                session.tab(false);
                session.activate();
                session.wait_for_presentation().unwrap();
                assert_eq!(session.text_at(1), "Seed planted!");
                // The immutable artifact and authoritative catalog remain unchanged.
                assert_eq!(bundle.cache_key(), key);
                assert_eq!(bundle.session_catalog().unwrap().fingerprint(), fingerprint);
            })
            .unwrap();
        }
    });
}
