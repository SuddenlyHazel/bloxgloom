//! Negotiated runtime metadata through production Network and the nonblocking
//! listener. Private scripts/state never become executable client registrations.
use super::*;
use crate::content::ContentManifest;
use bloxgloom_host_api::{actions::Target, gameplay::EventKind};

pub(super) fn packages(fixture: &Fixture) {
    fixture.package("library", "", "return function(_) end");
    fixture.package(
        "terrain",
        "requires bloxgloom:generation/v1\nmodule gen gen.luau",
        "return function(h) h.register_generator('terrain:land', 7, 'terrain:gen') end",
    );
    std::fs::write(
        fixture.0.join("packages/terrain/gen.luau"),
        "return function(_) end",
    )
    .unwrap();
    fixture.package("demo", "requires bloxgloom:actions/v1\nrequires bloxgloom:content/v1\nrequires bloxgloom:owner_systems/v1\ndependency library 1.0.0\nmodule decision decision.luau\nmodule clock clock.luau", r#"
        return function(h)
            h.register_item('demo:token', 'Token', 'bloxgloom:stone')
            h.register_entity('demo:counter', 3, 8, 2, 10)
            h.register_action('demo:use', 2, 'Use Token', 'item', 'demo:token', 'demo:decision')
            h.register_handler('demo:tick', 4, 'EntityTick', 'demo:counter', 'demo:decision')
            h.register_handler('demo:pickup', 5, 'PickupRequested', 'bloxgloom:drop', 'demo:decision')
            h.register_system { key='demo:clock', schema=2, revision=3, module='demo:clock',
                max_state_bytes=64, max_jobs_per_tick=1, read_world=false,
                seeds={{x=0,y=5,z=0,data='PRIVATE-SEED-DO-NOT-EXPORT'}} }
        end
    "#);
    std::fs::write(
        fixture.0.join("packages/demo/decision.luau"),
        "-- PRIVATE-SERVER-SOURCE\nreturn function() error('not a client callback') end",
    )
    .unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/clock.luau"),
        "return function(c) return c.data, 100000 end",
    )
    .unwrap();
}

#[test]
fn mixed_runtime_catalog_joins_restarts_and_remaps_saved_identities_without_execution() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    packages(&fixture);
    let global = crate::content::catalog().fingerprint();
    let initial = fixture.open().unwrap();
    let mut manifest = ContentManifest::from_catalog(&initial.world.catalog_arc());
    // Deliberately persist a legal non-default numeric assignment before any
    // custom entity/item exists. Tests all remap paths without altering sources
    // (the remap must not depend on behavior edits to select identities).
    for entry in &mut manifest.entries {
        if entry.key.starts_with("demo:") && matches!(entry.kind, b'E' | b'I' | b'G' | b'Y') {
            entry.id += 1000;
        }
    }
    manifest.entries.sort_by_key(|e| (e.kind, e.id));
    drop(initial);
    std::fs::write(
        fixture.0.join("save/content.map"),
        manifest.encode().unwrap(),
    )
    .unwrap();
    let mut key = None;
    for round in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        let expected = state.world.catalog_arc();
        let bundle = state.client_bundle.as_ref().unwrap();
        for private in ["PRIVATE-SEED-DO-NOT-EXPORT", "PRIVATE-SERVER-SOURCE"] {
            assert!(
                !bundle
                    .bytes()
                    .windows(private.len())
                    .any(|w| w == private.as_bytes())
            );
        }
        assert!(bundle.packages().values().all(|p| p.sources.is_empty()));
        if let Some(key) = key {
            assert_eq!(key, bundle.cache_key());
        }
        key = Some(bundle.cache_key());
        assert_eq!(ContentManifest::from_catalog(&expected), manifest);
        let token = expected.items().find(|i| i.key == "demo:token").unwrap().id;
        if round == 0 {
            let mut inventory = Inventory::default();
            assert_eq!(inventory.insert_with_catalog(token, 128, &expected), 0);
            state.inventory_store.save(0x720, &inventory).unwrap();
        }
        gameplay::serve(state, |address| {
            let catalog =
                crate::client::connect_inventory_probe(&address.to_string(), 0x720, token).unwrap();
            assert_eq!(ContentManifest::from_catalog(&catalog), manifest);
            assert_eq!(catalog.fingerprint(), expected.fingerprint());
            let action = catalog
                .discover_actions(&Target::Item("demo:token".into()))
                .find(|a| a.key == "demo:use")
                .unwrap();
            assert_eq!(action.label, "Use Token");
            assert_eq!(action.version, 2);
            assert!(catalog.entity_type_id_by_key("demo:counter").unwrap().0 >= 1000);
            assert!(
                catalog.gameplay_entity("demo:counter").is_none(),
                "no private state codec"
            );
            assert!(
                catalog
                    .gameplay_handler(EventKind::ActionRequested, "demo:use")
                    .is_none()
            );
            assert!(
                catalog
                    .gameplay_handler(EventKind::EntityTick, "demo:counter")
                    .is_none()
            );
            assert!(!catalog.owner_systems().any(|s| s.key == "demo:clock"));
            assert_eq!(crate::content::catalog().fingerprint(), global);
        });
    }
}

#[test]
fn generation_only_package_joins_and_restarts_without_client_generation() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.package(
        "demo",
        "requires bloxgloom:generation/v1\nmodule terrain terrain.luau",
        "return function(h) h.register_generator('demo:terrain', 9, 'demo:terrain') end",
    );
    std::fs::write(
        fixture.0.join("packages/demo/terrain.luau"),
        "return function(_) end",
    )
    .unwrap();
    for _ in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        let expected = state.world.catalog_arc();
        gameplay::serve(state, |address| {
            let catalog =
                crate::client::connect_catalog_probe(&address.to_string(), 0x721).unwrap();
            assert_eq!(catalog.fingerprint(), expected.fingerprint());
            assert_eq!(
                ContentManifest::from_catalog(&catalog),
                ContentManifest::from_catalog(&expected)
            );
        });
    }
}
