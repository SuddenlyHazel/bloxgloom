use super::*;

#[test]
fn actions_are_distinct_and_movement_keys_cannot_be_rebound() {
    let default = Bindings::default();
    assert!(default.valid());
    assert_eq!(default.action(KeyCode::KeyQ), Some(Action::Drop));
    assert_eq!(default.action(KeyCode::KeyW), None);
    let mut changed = default;
    changed.drop = parse("t").unwrap();
    assert!(changed.valid());
    assert_eq!(changed.action(KeyCode::KeyT), Some(Action::Drop));
    changed.kiln_fuel = KeyCode::KeyT;
    assert!(!changed.valid());
    changed.kiln_fuel = KeyCode::KeyW;
    assert!(!changed.valid());
}

#[test]
fn capture_rejects_builtin_named_and_movement_collisions_without_mutating() {
    let mut builtins = Bindings::default();
    let mut named = NamedBindings::default();
    assert!(named.bind("demo:wave", KeyCode::KeyT, builtins));
    assert!(!builtins.bind(Action::Drop, KeyCode::KeyT, &named));
    assert!(!builtins.bind(Action::Drop, KeyCode::KeyE, &named));
    assert!(!builtins.bind(Action::Drop, KeyCode::KeyW, &named));
    assert_eq!(builtins, Bindings::default());
    assert!(builtins.bind(Action::Drop, KeyCode::KeyY, &named));
    assert!(!named.bind("demo:wave", KeyCode::KeyY, builtins));
    assert_eq!(named.action(KeyCode::KeyT), Some("demo:wave"));
}
