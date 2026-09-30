//! Multiple ordinary actions freeze and negotiate together; bad sets fail atomically.
use super::*;

#[test]
fn package_action_set_preserves_delivery_bounds_and_duplicate_ownership_checks() {
    for (count, duplicate, accepted) in [(32, false, true), (33, false, false), (2, true, false)] {
        let fixture = Fixture::new();
        let catalog = Catalog::builtins();
        let items: Vec<_> = catalog.items().map(|item| item.key.as_ref()).collect();
        let mut source = String::from("return function(h) ");
        for index in 0..count {
            let identity = if duplicate { 1 } else { index + 1 };
            source.push_str(&format!("pcall(function() h.register_action('demo:action{identity}',1,'Action','item','{}','demo:action') end) ", items[index % items.len()]));
        }
        source.push_str("end");
        fixture.action(&source, "return function() end");
        let startup = fixture.startup(Arc::new(Catalog::builtins()));
        if !accepted {
            assert!(
                startup.is_err(),
                "invalid action set survived caught declaration error"
            );
            assert!(!fixture.0.join("save").exists());
            continue;
        }
        let state = crate::server::server_state_with_startup(
            7,
            fixture.0.join("save"),
            2,
            startup.unwrap(),
        )
        .unwrap();
        let delivered = state
            .client_bundle
            .as_ref()
            .unwrap()
            .session_catalog()
            .unwrap();
        let catalog = state.world.catalog();
        for index in 1..=count {
            let key = format!("demo:action{index}");
            assert_eq!(delivered.action(&key), catalog.action(&key));
        }
        assert_eq!(
            delivered
                .registered_actions()
                .filter(|action| action.key.starts_with("demo:"))
                .count(),
            32
        );
    }
}
