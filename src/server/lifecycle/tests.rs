use super::*;

#[test]
fn generation_registration_is_bounded_and_rejects_duplicate_keys() {
    struct Empty;
    impl bloxgloom_host_api::generation::Contributor for Empty {
        fn generate(
            &self,
            _: bloxgloom_host_api::generation::Context,
            _: &mut bloxgloom_host_api::generation::Output,
        ) -> Result<(), bloxgloom_host_api::generation::GenerationError> {
            Ok(())
        }
    }
    let mut registration = Registration::default();
    let entry = bloxgloom_host_api::generation::Registration {
        key: "sample:trees".into(),
        revision: 1,
        contributor: Arc::new(Empty),
    };
    registration.generation_contributor(entry.clone()).unwrap();
    let mut unversioned = entry.clone();
    unversioned.key = "sample:unversioned".into();
    unversioned.revision = 0;
    assert!(registration.generation_contributor(unversioned).is_err());
    assert!(registration.generation_contributor(entry).is_err());
    assert_eq!(registration.generation.len(), 1);
    assert!(
        registration
            .generation_contributor(bloxgloom_host_api::generation::Registration {
                key: "invalid".into(),
                revision: 1,
                contributor: Arc::new(Empty)
            })
            .is_err()
    );
}

#[test]
fn inventory_screen_metadata_survives_manifest_remapping_and_is_handshake_identity() {
    let mut catalog = Catalog::builtins();
    Registration::install(&bloxgloom_lifecycle_fixture::TallStore, &mut catalog).unwrap();
    let mut manifest = crate::content::ContentManifest::from_catalog(&catalog);
    let old = catalog
        .entity_type_id_by_key(bloxgloom_lifecycle_fixture::KEY)
        .unwrap();
    let entry = manifest
        .entries
        .iter_mut()
        .find(|e| e.kind == b'E' && e.key == bloxgloom_lifecycle_fixture::KEY)
        .unwrap();
    entry.id = 700;
    manifest.entries.sort_by_key(|e| (e.kind, e.id));
    let remapped = manifest.resolve_catalog(&catalog).unwrap();
    assert_eq!(
        remapped.inventory_screen(crate::content::EntityTypeId(700)),
        catalog.inventory_screen(old)
    );
    assert_eq!(
        remapped
            .inventory_for_state(
                remapped
                    .state_by_key(bloxgloom_lifecycle_fixture::KEY)
                    .unwrap()
            )
            .unwrap()
            .title,
        "TALL STORE"
    );
    let mut declarations = Registration::default();
    bloxgloom_lifecycle_fixture::TallStore
        .register(&mut declarations)
        .unwrap();
    declarations.screens[0].groups[0].insert = false;
    let mut changed = Catalog::builtins();
    changed.extension_cube(&declarations.cubes[0]).unwrap();
    changed
        .extension_storage(&declarations.definitions[0])
        .unwrap();
    changed
        .register_inventory_screen(declarations.screens.remove(0))
        .unwrap();
    assert!(
        manifest.resolve_catalog(&changed).is_err(),
        "permissions are part of the negotiated contract"
    );
}

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
        .with_extension(&Collision);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-lifecycle-collision-{}-{stamp}",
        std::process::id()
    ));
    assert!(startup.is_err());
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
