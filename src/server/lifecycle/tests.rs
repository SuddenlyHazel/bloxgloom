use super::*;

#[test]
fn lifecycle_collision_with_builtin_hook_is_rejected_before_world_creation() {
    struct Collision;
    impl Extension for Collision {
        fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
            let mut definition = super::super::entities::chest::definition();
            definition.entity = "fixture:hopper_override".into();
            definition.block = "bloxgloom:hopper".into();
            definition.placement_item = "bloxgloom:hopper".into();
            definition.anchor_state = "bloxgloom:hopper".into();
            definition.footprint[0].state = "bloxgloom:hopper".into();
            r.storage_block_entity(definition)
        }
    }
    let startup = super::super::startup::ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_extension(&Collision)
        .unwrap();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lifecycle-collision-{}-{stamp}",
        std::process::id()
    ));
    assert!(super::super::server_state_with_startup(7, save.clone(), 8, startup).is_err());
    assert!(
        !save.exists(),
        "registration failure must precede save creation"
    );
}

#[test]
fn failed_external_registration_is_atomic_and_changed_lifecycle_changes_manifest() {
    let mut catalog = Catalog::builtins();
    let before = catalog.fingerprint();
    struct Invalid;
    impl Extension for Invalid {
        fn register(&self, r: &mut dyn Registrar) -> Result<(), RegistrationError> {
            bloxgloom_lifecycle_fixture::TallStore.register(r)?;
            let mut duplicate = super::super::entities::chest::definition();
            duplicate.entity = "fixture:missing".into();
            duplicate.block = "fixture:missing".into();
            r.storage_block_entity(duplicate)
        }
    }
    assert!(Registration::install(&Invalid, &mut catalog).is_err());
    assert_eq!(catalog.fingerprint(), before);
    Registration::install(&bloxgloom_lifecycle_fixture::TallStore, &mut catalog).unwrap();
    let installed = catalog.fingerprint();
    assert!(Registration::install(&bloxgloom_lifecycle_fixture::TallStore, &mut catalog).is_err());
    assert_eq!(catalog.fingerprint(), installed);
    let mut declarations = Registration::default();
    bloxgloom_lifecycle_fixture::TallStore
        .register(&mut declarations)
        .unwrap();
    let mut changed = Catalog::builtins();
    changed.extension_cube(&declarations.cubes[0]).unwrap();
    declarations.definitions[0].slots = 10;
    changed
        .extension_storage(&declarations.definitions[0])
        .unwrap();
    assert_ne!(changed.fingerprint(), installed);
    let mut invalid = super::super::entities::chest::definition();
    invalid.footprint.push(invalid.footprint[0].clone());
    assert!(Registry::resolve(&catalog, &[invalid]).is_err());
}
