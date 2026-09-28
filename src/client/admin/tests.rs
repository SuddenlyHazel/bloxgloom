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
