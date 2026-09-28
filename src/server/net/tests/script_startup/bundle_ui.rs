//! Real nonblocking listener -> client install/cache -> local UI session.
use super::*;

#[test]
fn package_ui_prepares_on_join_and_cached_reconnect_without_client_execution() {
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
            let bundle = crate::client::connect_bundle_probe(&address.to_string(), profile)
                .unwrap()
                .unwrap();
            assert_eq!(bundle.cache_key(), key);
            let mut session = crate::ui::authored::Session::new(Arc::clone(bundle.ui().unwrap()));
            session.resize(640, 360, 1.0);
            assert!(session.focused_id().is_none());
            session.tab(false);
            assert_eq!(session.focused_id(), Some("uidemo:welcome/name"));
            assert_eq!(session.event(), Some("uidemo:name-changed"));
            session.edit(false, Some(" local edit"));
            // The immutable artifact and authoritative catalog remain unchanged.
            assert_eq!(bundle.cache_key(), key);
            assert_eq!(bundle.session_catalog().unwrap().fingerprint(), fingerprint);
        }
    });
}
