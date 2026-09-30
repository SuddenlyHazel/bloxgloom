use super::*;

fn declaration(key: &str, default: &str) -> Declaration {
    Declaration {
        key: key.into(),
        event: "demo:activate".into(),
        default: default.into(),
        scope: Scope::Game,
        open: false,
    }
}

#[test]
fn declared_input_rejects_native_keys_ownership_duplicates_and_bounds() {
    for default in ["W", "A", "S", "D", "E", "Q", "R", "F", "F6", "BB"] {
        assert!(resolve("demo", Some(vec![declaration("demo:toggle", default)])).is_err());
    }
    assert!(resolve("demo", Some(vec![declaration("other:toggle", "B")])).is_err());
    assert!(
        resolve(
            "demo",
            Some(vec![
                declaration("demo:toggle", "B"),
                declaration("demo:toggle", "T")
            ])
        )
        .is_err()
    );
    assert!(
        resolve(
            "demo",
            Some(vec![
                declaration("demo:toggle", "B"),
                declaration("demo:other", "B")
            ])
        )
        .is_err()
    );
    assert!(
        resolve(
            "demo",
            Some(
                (0..17)
                    .map(|i| declaration(&format!("demo:key{i}"), "B"))
                    .collect()
            )
        )
        .is_err()
    );
}

#[test]
fn declared_input_user_override_wins_and_invalid_collisions_disable_default() {
    let binding = resolve("demo", Some(vec![declaration("demo:toggle", "B")]))
        .unwrap()
        .remove(0);
    let builtins = Bindings::default();
    let mut named = NamedBindings::default();
    assert_eq!(binding.effective_key(&named, builtins), Some(KeyCode::KeyB));
    assert!(named.bind("demo:toggle", KeyCode::KeyT, builtins));
    assert_eq!(binding.effective_key(&named, builtins), Some(KeyCode::KeyT));
    named.0.insert("demo:toggle".into(), KeyCode::KeyW);
    assert_eq!(binding.effective_key(&named, builtins), None);
    named.0.remove("demo:toggle");
    assert!(named.bind("other:command", KeyCode::KeyB, builtins));
    assert_eq!(binding.effective_key(&named, builtins), None);
    named.0.clear();
    let mut reassigned = builtins;
    assert!(reassigned.bind(bindings::Action::Inventory, KeyCode::KeyB, &named));
    assert_eq!(binding.effective_key(&named, reassigned), None);
}

#[test]
fn declared_input_scopes_follow_current_screen() {
    let mut binding = resolve("demo", Some(vec![declaration("demo:toggle", "B")]))
        .unwrap()
        .remove(0);
    assert!(binding.active(false));
    assert!(!binding.active(true));
    binding.scope = Scope::Ui;
    assert!(!binding.active(false));
    assert!(binding.active(true));
    binding.scope = Scope::Both;
    assert!(binding.active(false));
    assert!(binding.active(true));
}
