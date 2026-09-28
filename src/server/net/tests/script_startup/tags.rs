//! Luau tag contributions rebuild identically on a verified listener catalog.
use super::*;

#[test]
fn package_tags_negotiate_nested_members_and_reject_changed_save_identity() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let untagged = Fixture::new();
    untagged.package(
        "demo",
        CONTENT,
        "return function(h) h.register_item('demo:token','Token','bloxgloom:stone') end",
    );
    let untagged = untagged.open().unwrap();
    assert!(
        untagged
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x07")
    );
    let fixture = Fixture::new();
    fixture.package(
        "demo",
        CONTENT,
        "return function(h)
        h.register_item('demo:token','Token','bloxgloom:stone')
        h.register_tag('demo:base','item',{'demo:token','bloxgloom:stick'})
        h.register_tag('demo:all','item',{'#demo:base','bloxgloom:seeds'})
    end",
    );
    let state = Box::new(fixture.open().unwrap());
    let server = state.world.catalog_arc();
    let members = server.composition.item_tag("demo:all").unwrap();
    for member in ["demo:token", "bloxgloom:stick", "bloxgloom:seeds"] {
        assert!(members.contains(member));
    }
    let fingerprint = server.fingerprint();
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x18")
    );
    gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x728).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        assert_eq!(client.composition.item_tag("demo:all"), Some(members));
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    let before = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    fixture.package(
        "demo",
        CONTENT,
        "return function(h)
        h.register_item('demo:token','Token','bloxgloom:stone')
        h.register_tag('demo:base','item',{'demo:token'})
        h.register_tag('demo:all','item',{'#demo:base','bloxgloom:seeds'})
    end",
    );
    assert!(fixture.open().is_err());
    assert_eq!(
        std::fs::read(fixture.0.join("save/content.map")).unwrap(),
        before
    );
}

#[test]
fn caught_invalid_tags_do_not_publish_partial_catalogs() {
    for declaration in [
        "h.register_tag('other:tag','item',{'bloxgloom:stick'})",
        "h.register_tag('demo:tag','item',{'#demo:missing'})",
        "h.register_tag('demo:tag','item',{'bloxgloom:stick','bloxgloom:stick'})",
        "h.register_tag('demo:tag','item',{'demo:tag'})",
        "h.register_tag('demo:tag','item',{'bloxgloom:stick',[33]='bloxgloom:seeds'})",
    ] {
        let fixture = Fixture::new();
        fixture.package(
            "demo",
            CONTENT,
            &format!("return function(h) pcall(function() {declaration} end) end"),
        );
        assert!(fixture.open().is_err(), "{declaration}");
        assert!(!fixture.0.join("save").exists());
    }
}
