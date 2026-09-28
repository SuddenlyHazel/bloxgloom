use super::*;
use bloxgloom_host_api::actions::{
    Action, Command as Descriptor, CommandArgument, CommandPermission, Operation, Target,
};

fn request(input: &str, catalog: &Catalog) -> Request {
    let Command::Registered(request) = parse(input, catalog).unwrap() else {
        panic!("expected request")
    };
    request
}

#[test]
fn native_aliases_and_namespaced_commands_use_the_same_canonical_request() {
    let catalog = Catalog::builtins();
    let key = catalog.items().next().unwrap().key.to_string();
    assert_eq!(
        request(&format!("/give {key} 12"), &catalog),
        request(&format!("bloxgloom:admin_give {key} 12"), &catalog)
    );
    assert_eq!(
        request(&format!("give {key}"), &catalog),
        request(&format!("give {key} 128"), &catalog)
    );
    assert!(parse(&format!("give {key} 129"), &catalog).is_err());
    assert!(parse("give madeup:block", &catalog).is_err());
    assert_eq!(
        request("/spawn mossbun", &catalog),
        request("bloxgloom:admin_spawn bloxgloom:mossbun", &catalog)
    );
    assert!(parse("spawn mossbun 100", &catalog).is_err());
    assert!(parse("spawn madeup", &catalog).is_err());
    assert!(parse("give mossbun", &catalog).is_err());
    assert!(matches!(parse("help", &catalog), Ok(Command::Help)));
    assert!(parse("help extra", &catalog).is_err());
}

#[test]
fn generic_parser_uses_ordered_negotiated_schema_not_builtin_names() {
    let mut catalog = Catalog::builtins();
    catalog
        .register_action(Action {
            key: "demo:combine".into(),
            version: 7,
            label: "Combine".into(),
            target: Target::Empty,
            operation: Operation::Gameplay,
            panel: None,
            command: Some(Descriptor {
                permission: CommandPermission::Player,
                arguments: vec![
                    CommandArgument::EntityKey { max_bytes: 48 },
                    CommandArgument::ItemKey { max_bytes: 48 },
                    CommandArgument::Count { default: Some(4) },
                ],
            }),
        })
        .unwrap();
    let request = request("demo:combine bloxgloom:mossbun bloxgloom:stick", &catalog);
    assert_eq!(request.version, 7);
    assert_eq!(request.arguments.last(), Some(&4));
    assert!(parse("demo:combine bloxgloom:stick bloxgloom:mossbun", &catalog).is_err());
    assert!(parse("bloxgloom:slot_move", &catalog).is_err());
    assert!(parse(&"x".repeat(1025), &catalog).is_err());
}

#[test]
fn binding_candidates_require_zero_arg_empty_target_and_keep_absent_local_entries() {
    let mut catalog = Catalog::builtins();
    for (key, arguments) in [
        ("demo:wave", vec![]),
        (
            "demo:needs_item",
            vec![CommandArgument::ItemKey { max_bytes: 48 }],
        ),
    ] {
        catalog
            .register_action(Action {
                key: key.into(),
                version: 1,
                label: key.into(),
                target: Target::Empty,
                operation: Operation::Gameplay,
                panel: None,
                command: Some(Descriptor {
                    permission: CommandPermission::Player,
                    arguments,
                }),
            })
            .unwrap();
    }
    let mut named = bindings::NamedBindings::default();
    assert!(named.bind(
        "other:remembered",
        KeyCode::KeyT,
        bindings::Bindings::default()
    ));
    let targets = binding_targets(&catalog, &named);
    assert_eq!(
        &targets[..4],
        &[
            BindingTarget::Builtin(Builtin::Inventory),
            BindingTarget::Builtin(Builtin::KilnInput),
            BindingTarget::Builtin(Builtin::KilnFuel),
            BindingTarget::Builtin(Builtin::Drop),
        ]
    );
    assert!(targets.contains(&BindingTarget::Named("demo:wave".into())));
    assert!(targets.contains(&BindingTarget::Absent("other:remembered".into())));
    assert!(
        !targets
            .iter()
            .any(|target| matches!(target, BindingTarget::Named(key) if key == "demo:needs_item"))
    );
}

#[test]
fn capture_only_changes_valid_local_targets_and_leaves_absent_session_binding_inert() {
    let mut config = crate::config::Config::default();
    assert!(!BindingTarget::Absent("other:remembered".into()).capture(KeyCode::KeyT, &mut config));
    assert!(BindingTarget::Named("demo:wave".into()).capture(KeyCode::KeyT, &mut config));
    assert!(!BindingTarget::Builtin(Builtin::Drop).capture(KeyCode::KeyT, &mut config));
    assert!(!BindingTarget::Builtin(Builtin::Drop).capture(KeyCode::KeyW, &mut config));
    assert!(BindingTarget::Builtin(Builtin::Drop).capture(KeyCode::KeyY, &mut config));
    assert_eq!(
        config.named_bindings.action(KeyCode::KeyT),
        Some("demo:wave")
    );
    assert_eq!(config.bindings.drop, KeyCode::KeyY);
}
#[test]
fn appearance_command_selects_only_negotiated_palettes_without_a_gameplay_request() {
    let mut catalog = Catalog::builtins();
    assert!(matches!(
        parse("appearance 0 1 2", &catalog),
        Ok(Command::Appearance([0, 1, 2]))
    ));
    for input in [
        "appearance 6 8 6",
        "appearance 1 2",
        "appearance 1 2 3 4",
        "appearance 0.5 0 0",
        "appearance #ff00aa 0 0",
        "appearance 255 0 0",
        "appearance -1 0 0",
    ] {
        assert!(parse(input, &catalog).is_err(), "{input}");
    }
    catalog
        .register_player_appearance(bloxgloom_host_api::appearance::Appearance {
            key: "demo:wardrobe".into(),
            revision: 1,
            model: bloxgloom_host_api::appearance::MODEL.into(),
            palettes: [vec![[0.2; 3]], vec![[0.4; 3]], vec![[0.6; 3]]],
        })
        .unwrap();
    assert!(matches!(
        parse("/appearance 6 8 6", &catalog),
        Ok(Command::Appearance([6, 8, 6]))
    ));
    assert!(parse("appearance 7 8 6", &catalog).is_err());
}
