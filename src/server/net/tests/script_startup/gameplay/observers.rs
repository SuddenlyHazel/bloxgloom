//! Real commit delivery remains independent of bounded, faulty Lua observers.
use super::*;
use bloxgloom_host_api::gameplay::{Committed, Observer, ObserverRegistration};

struct Witness(mpsc::SyncSender<Committed>);
impl Observer for Witness {
    fn on_commit(&self, event: &Committed) {
        self.0.try_send(event.clone()).unwrap();
    }
}

#[test]
fn luau_committed_observer_timeout_cannot_block_receipt_or_later_observer_over_listener() {
    let fixture = Fixture::new();
    fixture.package(
        "demo",
        "requires bloxgloom:actions/v1\nmodule action action.luau\nmodule observer observer.luau",
        r#"
        return function(h)
            h.register_action('demo:shift',1,'Grant','empty',nil,'demo:action')
            h.register_committed_observer('demo:audit',1,'demo:observer')
        end
    "#,
    );
    std::fs::write(
        fixture.0.join("packages/demo/action.luau"),
        "return function(c) assert(c.give('player',{item='bloxgloom:stick',count=1})) end",
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/observer.luau"), "-- PRIVATE-OBSERVER-SOURCE\nreturn function(e) assert(e.kind=='Committed' and e.inventory and e.inventory.slots==nil); while true do end end").unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let bundle = state.client_bundle.as_ref().unwrap();
    assert!(
        !bundle
            .bytes()
            .windows(b"PRIVATE-OBSERVER-SOURCE".len())
            .any(|w| w == b"PRIVATE-OBSERVER-SOURCE")
    );
    let inert = bundle.session_catalog().unwrap();
    let manifest = crate::content::ContentManifest::from_catalog(&catalog);
    let resolved = manifest.resolve_catalog(&inert).unwrap();
    assert_eq!(resolved.fingerprint(), catalog.fingerprint());
    assert_eq!(
        resolved.gameplay_observers().count(),
        0,
        "client gained server callback"
    );
    // Add a native witness to the delivery lane alone, without altering the
    // frozen negotiated catalog. It runs after the real script observer.
    let (sender, receiver) = mpsc::sync_channel(4);
    let mut observers = Catalog::builtins();
    for registration in catalog.gameplay_observers() {
        observers
            .register_gameplay_observer((**registration).clone())
            .unwrap();
    }
    observers
        .register_gameplay_observer(ObserverRegistration {
            key: "witness:commit".into(),
            version: 1,
            observer: Arc::new(Witness(sender)),
        })
        .unwrap();
    state.notifications = crate::server::notifications::Lane::new(&observers).unwrap();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        peer.inventory_at(1);
        let event = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(event.inventory, Some((PROFILE, peer.inventory.revision)));
        // The exhausted VM does not poison the next invocation or the lane.
        let request = peer.request(0);
        assert!(peer.send(&request).0);
        peer.inventory_at(2);
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .inventory,
            Some((PROFILE, peer.inventory.revision))
        );
    });
}

#[test]
fn caught_invalid_committed_observer_registration_still_refuses_startup() {
    for requires in ["", "requires bloxgloom:actions/v1"] {
        let fixture = Fixture::new();
        fixture.package("demo", requires, "return function(h) pcall(function() h.register_committed_observer('foreign:observe',1,'demo:main') end) end");
        assert!(fixture.open().is_err());
        assert!(
            !fixture.0.join("save").exists(),
            "invalid observer opened save"
        );
    }
}
