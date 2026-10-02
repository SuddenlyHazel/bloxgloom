use super::*;

#[test]
fn command_facet_participates_in_catalog_and_saved_player_identity() {
    let descriptor = |permission: Option<CommandPermission>| Action {
        key: "test:command".into(),
        version: 1,
        label: "Command".into(),
        target: Target::Empty,
        operation: Operation::Gameplay,
        panel: None,
        command: permission.map(|permission| Command {
            aliases: Vec::new(),
            permission,
            arguments: vec![],
        }),
    };
    let mut fingerprints = Vec::new();
    let mut player_identities = Vec::new();
    for permission in [
        None,
        Some(CommandPermission::Player),
        Some(CommandPermission::Admin),
    ] {
        let mut catalog = Catalog::builtins();
        let action = descriptor(permission);
        catalog.register_action(action.clone()).unwrap();
        assert_eq!(catalog.action("test:command").unwrap().as_ref(), &action);
        fingerprints.push(catalog.fingerprint());
        let manifest = crate::content::ContentManifest::from_catalog(&catalog);
        player_identities.push(
            manifest
                .entries
                .into_iter()
                .find(|e| e.kind == b'E' && e.key == "bloxgloom:player")
                .unwrap(),
        );
    }
    for i in 0..3 {
        for j in i + 1..3 {
            assert_ne!(fingerprints[i], fingerprints[j]);
            assert_ne!(player_identities[i], player_identities[j]);
        }
    }
}
