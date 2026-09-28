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
