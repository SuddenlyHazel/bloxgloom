use super::*;

#[test]
fn command_metadata_roundtrips_schema_and_permissions_and_rejects_forgery() {
    let requires = vec![composition::ACTIONS.into()];
    for command in [
        None,
        Some(Command {
            permission: CommandPermission::Player,
            arguments: vec![],
        }),
        Some(Command {
            permission: CommandPermission::Admin,
            arguments: vec![
                CommandArgument::ItemKey { max_bytes: 64 },
                CommandArgument::EntityKey { max_bytes: 48 },
                CommandArgument::Count { default: Some(128) },
            ],
        }),
    ] {
        let action = Action {
            key: "demo:command".into(),
            version: 1,
            label: "Command".into(),
            target: Target::Empty,
            operation: Operation::Gameplay,
            panel: None,
            command,
        };
        let runtime = Runtime {
            actions: vec![action.clone()],
            ..Runtime::default()
        };
        let mut encoded = Writer(Vec::new());
        runtime.encode_package(&mut encoded, "demo").unwrap();
        let mut prefix = Writer(Vec::new());
        prefix.count(1).unwrap();
        prefix.field(b"demo:command").unwrap();
        prefix.count(1).unwrap();
        prefix.field(b"Command").unwrap();
        let kind_offset = prefix.0.len();
        if action.command.is_none() {
            // Ordinary empty-target actions keep their previous wire format.
            for _ in 0..5 {
                prefix.count(0).unwrap();
            }
            assert_eq!(encoded.0, prefix.0);
        }
        let mut decoded = Runtime::default();
        let mut reader = Reader(&encoded.0);
        decoded
            .decode_package(&mut reader, "demo", &requires)
            .unwrap();
        assert!(reader.0.is_empty());
        assert_eq!(decoded.actions, vec![action.clone()]);
        assert_eq!(
            decoded.actions[0].fingerprint_bytes(),
            action.fingerprint_bytes()
        );
        assert!(
            Runtime::default()
                .decode_package(&mut Reader(&encoded.0), "demo", &[])
                .is_err()
        );
        for bad in [3_u32, 4, 7] {
            let mut forged = encoded.0.clone();
            forged[kind_offset..kind_offset + 4].copy_from_slice(&bad.to_le_bytes());
            assert!(
                Runtime::default()
                    .decode_package(&mut Reader(&forged), "demo", &requires)
                    .is_err()
            );
        }
        if action
            .command
            .as_ref()
            .is_some_and(|c| !c.arguments.is_empty())
        {
            // Count, type tag, and key bound are independently fail-closed.
            for (offset, bad) in [(4, 9_u32), (8, 3), (12, 0), (12, 129)] {
                let mut forged = encoded.0.clone();
                forged[kind_offset + offset..kind_offset + offset + 4]
                    .copy_from_slice(&bad.to_le_bytes());
                assert!(
                    Runtime::default()
                        .decode_package(&mut Reader(&forged), "demo", &requires)
                        .is_err()
                );
            }
        }
    }
}
