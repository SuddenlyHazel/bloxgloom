//! Exact downloaded catalog and live client callback preparation over real TCP.
use super::*;
#[test]
fn item_visuals_icons_and_stack_callbacks_negotiate_through_nonblocking_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/item-visuals/packages/visual");
    let target = fixture.0.join("packages/visual");
    std::fs::create_dir_all(target.join("server")).unwrap();
    std::fs::create_dir_all(target.join("client")).unwrap();
    for name in [
        "package.txt",
        "server/main.luau",
        "client/startup.luau",
        "client/cell.luau",
    ] {
        std::fs::copy(source.join(name), target.join(name)).unwrap();
    }
    let state = Box::new(fixture.open().unwrap());
    let manifest = crate::content::ContentManifest::from_catalog(state.world.catalog());
    gameplay::serve(state, |address| {
        let catalog = crate::client::connect_catalog_probe(&address.to_string(), 0x1c07).unwrap();
        assert_eq!(
            crate::content::ContentManifest::from_catalog(&catalog),
            manifest
        );
        let stack = crate::inventory::Stack::new(catalog.item_by_key("visual:cell").unwrap(), 128);
        assert_eq!(catalog.item_icon(stack.item).unwrap().rows[2], ".x..x.");
        let visual = crate::client::item_visuals::tests::wait(&catalog, &stack);
        assert_eq!(visual.drop_scale, 1.5);
        assert_eq!(visual.icon.as_ref().unwrap().rows[2], ".x++x.");
    });
}
