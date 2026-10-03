use super::*;
use crate::inventory::{Inventory, InventoryStore, Stack};
use bloxgloom_host_api::{Extension, Registrar, composition::*, content::*};
use bloxgloom_lifecycle_fixture::content::{CHIP, Content, LAMP, REED};

fn fixture() -> Catalog {
    crate::server::catalog_with_extension(Catalog::builtins(), &Content).unwrap()
}

#[test]
fn drop_size_defaults_and_nondefault_identity_survive_remapping() {
    let mut base = Catalog::builtins();
    let mut item = Item {
        key: "test:drop_size".into(),
        name: "DROP".into(),
        swatch: [1.0; 4],
        texture: "bloxgloom:stone".into(),
        placeable: None,
        sprite: true,
        drop_size: DropSize::Normal,
        drop_animation: Default::default(),
        drop_policy: Default::default(),
        components: Components::None,
    };
    base.public_item(&item).unwrap();
    let id = base.item_by_key(&item.key).unwrap();
    assert_eq!(base.drop_size(id), DropSize::Normal);
    let original = base
        .identities()
        .into_iter()
        .find(|e| e.0 == b'I' && e.2 == item.key)
        .unwrap()
        .3;
    let mut sized = Catalog::builtins();
    item.drop_size = DropSize::Large;
    sized.public_item(&item).unwrap();
    assert_ne!(sized.fingerprint(), base.fingerprint());
    assert_ne!(
        sized
            .identities()
            .into_iter()
            .find(|e| e.0 == b'I' && e.2 == item.key)
            .unwrap()
            .3,
        original
    );
    assert!(
        ContentManifest::from_catalog(&base)
            .resolve_catalog(&sized)
            .is_err()
    );
    let manifest = ContentManifest::from_catalog(&sized);
    let resolved = manifest.resolve_catalog(&sized).unwrap();
    assert_eq!(
        resolved.drop_size(resolved.item_by_key(&item.key).unwrap()),
        DropSize::Large
    );
    item.drop_size = DropSize::Small;
    let mut small = Catalog::builtins();
    small.public_item(&item).unwrap();
    assert_ne!(small.fingerprint(), sized.fingerprint());
    assert_eq!(item.drop_size.multiplier(), 0.75);
    assert_eq!(DropSize::Normal.multiplier(), 1.0);
    assert_eq!(DropSize::Large.multiplier(), 1.25);
}

#[test]
fn drop_policy_is_validated_fingerprinted_and_remapped() {
    let mut item = Item {
        key: "test:policy".into(),
        name: "Policy".into(),
        swatch: [1.0; 4],
        texture: "bloxgloom:stone".into(),
        placeable: None,
        sprite: true,
        drop_size: Default::default(),
        drop_animation: Default::default(),
        drop_policy: Default::default(),
        components: Components::None,
    };
    let mut baseline = Catalog::builtins();
    baseline.public_item(&item).unwrap();
    for invalid in [
        DropPolicy {
            gravity: f32::NAN,
            ..Default::default()
        },
        DropPolicy {
            radius: 0.5,
            ..Default::default()
        },
        DropPolicy {
            pickup_range: 9.0,
            ..Default::default()
        },
        DropPolicy {
            merge_range: -0.0,
            ..Default::default()
        },
        DropPolicy {
            lifetime_ms: 999,
            ..Default::default()
        },
        DropPolicy {
            terminal_speed: f32::INFINITY,
            ..Default::default()
        },
    ] {
        item.drop_policy = invalid;
        assert!(Catalog::builtins().public_item(&item).is_err());
        assert!(DropPolicy::from_bytes(invalid.to_bytes()).is_none());
    }
    item.drop_policy = DropPolicy {
        gravity: 12.0,
        terminal_speed: 4.0,
        radius: 0.4,
        pickup_range: 4.0,
        merge_range: 3.0,
        lifetime_ms: 2_000,
    };
    assert_eq!(
        DropPolicy::from_bytes(item.drop_policy.to_bytes()),
        Some(item.drop_policy)
    );
    let mut custom = Catalog::builtins();
    custom.public_item(&item).unwrap();
    assert_eq!(baseline.max_drop_pickup_range(), 2.25);
    assert_eq!(custom.max_drop_pickup_range(), 4.0);
    assert_ne!(baseline.fingerprint(), custom.fingerprint());
    assert!(
        ContentManifest::from_catalog(&baseline)
            .resolve_catalog(&custom)
            .is_err()
    );
    let mut remapped = ContentManifest::from_catalog(&custom);
    remapped
        .entries
        .iter_mut()
        .find(|e| e.kind == b'I' && e.key == item.key)
        .unwrap()
        .id += 10;
    let resolved = remapped.resolve_catalog(&custom).unwrap();
    assert_eq!(resolved.max_drop_pickup_range(), 4.0);
    assert_eq!(
        resolved.drop_policy(resolved.item_by_key(&item.key).unwrap()),
        item.drop_policy
    );
}

#[test]
fn drop_animation_is_validated_and_remapped_by_item_key() {
    let mut item = Item {
        key: "test:animated".into(),
        name: "Animated".into(),
        swatch: [1.0; 4],
        texture: "bloxgloom:stone".into(),
        placeable: None,
        sprite: true,
        drop_size: DropSize::Normal,
        drop_animation: DropAnimation::default(),
        drop_policy: Default::default(),
        components: Components::None,
    };
    let mut plain = Catalog::builtins();
    plain.public_item(&item).unwrap();
    let original = plain.fingerprint();
    item.drop_animation.pickup_duration = f32::NAN;
    assert!(Catalog::builtins().public_item(&item).is_err());
    item.drop_animation.pickup_duration = 10.0;
    assert!(Catalog::builtins().public_item(&item).is_err());
    item.drop_animation.pickup_duration = 0.8;
    let mut animated = Catalog::builtins();
    animated.public_item(&item).unwrap();
    assert_ne!(animated.fingerprint(), original);
    assert!(
        ContentManifest::from_catalog(&plain)
            .resolve_catalog(&animated)
            .is_err()
    );
    let mut remapped = ContentManifest::from_catalog(&animated);
    let entry = remapped
        .entries
        .iter_mut()
        .find(|e| e.kind == b'I' && e.key == item.key)
        .unwrap();
    entry.id += 10;
    let resolved = remapped.resolve_catalog(&animated).unwrap();
    assert_eq!(
        resolved.drop_animation(resolved.item_by_key(&item.key).unwrap()),
        item.drop_animation
    );
}

#[test]
fn public_content_compiles_and_all_metadata_survives_manifest_remapping() {
    let catalog = fixture();
    let state = catalog
        .state_by_key(&format!("{LAMP}[axis=x,lit=false]"))
        .unwrap();
    let lit = catalog.state_with_property(state, "lit", "true").unwrap();
    assert_eq!(catalog.emission(state), 0);
    assert_eq!(catalog.emission(lit), 13);
    assert_eq!(
        catalog.state(state).unwrap().face_texture(0, 1),
        Some(catalog.state(state).unwrap().textures.top)
    );
    assert_ne!(
        catalog.state(state).unwrap().textures.top,
        catalog.state(state).unwrap().textures.side
    );
    let reed = catalog.state_by_key(REED).unwrap();
    assert_eq!(catalog.block_flags(reed) & (SOLID | OPAQUE), 0);
    assert_ne!(
        catalog.block_flags(reed) & (PLANT | CUTOUT | REPLACEABLE | FLAMMABLE),
        0
    );
    assert_eq!(catalog.plant_selection_margin(reed), 0.35);
    let kiln = catalog.machine(KILN_ENTITY_TYPE).unwrap();
    assert!(
        kiln.process
            .as_ref()
            .unwrap()
            .fuels
            .iter()
            .any(|f| f.item == REED && f.pulses == 60)
    );
    assert!(kiln.filters[0].items.iter().any(|key| key == REED));

    let mut manifest = ContentManifest::from_catalog(&catalog);
    for entry in &mut manifest.entries {
        if entry.key.starts_with("fixture:") {
            entry.id += 65_536;
        }
    }
    manifest.entries.sort_by_key(|e| (e.kind, e.id));
    let manifest = ContentManifest::decode(&manifest.encode().unwrap()).unwrap();
    let resolved = manifest.resolve_catalog(&catalog).unwrap();
    assert_eq!(ContentManifest::from_catalog(&resolved), manifest);
    assert_eq!(
        resolved.plant_selection_margin(resolved.state_by_key(REED).unwrap()),
        0.35
    );
    let chip = resolved.items().find(|i| i.key == CHIP).unwrap().id;
    assert_eq!(resolved.item_icon(chip).unwrap().rows[0], ".cccc.");
    let mut changed = catalog.clone();
    changed.item_icons.remove(CHIP);
    assert_ne!(changed.fingerprint(), catalog.fingerprint());
    assert!(
        Stack::with_components(chip, 2, 2, vec![3, 9])
            .unwrap()
            .valid_in(&resolved)
    );
    assert!(
        !Stack::with_components(chip, 2, 1, vec![3, 9])
            .unwrap()
            .valid_in(&resolved)
    );
    assert_eq!(
        resolved
            .composition
            .item_tag("fixture:copper")
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        resolved.fingerprint(),
        manifest.resolve_catalog(&catalog).unwrap().fingerprint()
    );
}

#[test]
fn registered_component_schemas_guard_inventory_and_container_codecs() {
    let catalog = fixture();
    let chip = catalog.items().find(|i| i.key == CHIP).unwrap().id;
    let lamp = catalog.items().find(|i| i.key == LAMP).unwrap().id;
    let stack = Stack::with_components(chip, 128, 2, vec![3, 9]).unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(stack.clone());
    let encoded = InventoryStore::encode_snapshot_with_catalog(&inventory, &catalog).unwrap();
    assert_eq!(
        InventoryStore::decode_snapshot_with_catalog(&encoded, &catalog).unwrap(),
        inventory
    );
    let encoded = crate::inventory::container::encode(&[Some(stack.clone())], &catalog).unwrap();
    assert_eq!(
        crate::inventory::container::decode(&encoded, 1, &catalog).unwrap(),
        [Some(stack)]
    );
    for invalid in [
        Stack::with_components(chip, 1, 1, vec![3]).unwrap(),
        Stack::with_components(chip, 1, 2, vec![3; 17]).unwrap(),
        Stack::with_components(lamp, 1, 2, vec![3]).unwrap(),
        Stack::new(chip, 129),
    ] {
        inventory.slots[0] = Some(invalid.clone());
        assert!(InventoryStore::encode_snapshot_with_catalog(&inventory, &catalog).is_err());
        assert!(crate::inventory::container::encode(&[Some(invalid)], &catalog).is_err());
    }
    // Well-framed but incompatible component bytes must fail decode too.
    let mut wrong_version = encoded;
    wrong_version[12..14].copy_from_slice(&1u16.to_le_bytes());
    assert!(crate::inventory::container::decode(&wrong_version, 1, &catalog).is_err());
}

struct Contribute;
impl Extension for Contribute {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), Error> {
        r.package(Package {
            key: "other:interop".into(),
            version: 1,
            dependencies: vec![Dependency {
                package: "fixture:content".into(),
                version: 1,
            }],
            requires: vec![CONTENT.into()],
        })?;
        r.tag(Tag {
            key: "fixture:copper".into(),
            kind: TagKind::Item,
            members: vec![TagMember::Definition(REED.into())],
        })
    }
}

#[test]
fn bundle_dependencies_and_tag_union_are_independent_of_extension_order() {
    let first: [&dyn Extension; 2] = [&Content, &Contribute];
    let second: [&dyn Extension; 2] = [&Contribute, &Content];
    let mut a = Catalog::builtins();
    let mut b = a.clone();
    a = crate::server::catalog_with_extension(a, &Bundle(&first)).unwrap();
    b = crate::server::catalog_with_extension(b, &Bundle(&second)).unwrap();
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.composition.item_tag("fixture:copper").unwrap().len(), 3);
    let before = a.fingerprint();
    assert!(crate::server::catalog_with_extension(a.clone(), &Contribute).is_err());
    assert_eq!(before, a.fingerprint());
}

struct Declare(fn(&mut dyn Registrar) -> Result<(), Error>);
impl Extension for Declare {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), Error> {
        self.0(r)
    }
}
fn package(key: &str, dependency: &str) -> Package {
    Package {
        key: key.into(),
        version: 1,
        dependencies: vec![Dependency {
            package: dependency.into(),
            version: 1,
        }],
        requires: vec![],
    }
}

#[test]
fn invalid_composition_and_missing_content_fail_atomically_before_installation() {
    let invalid = [
        Declare(|r| {
            r.package(package("test:a", "test:b"))?;
            r.package(package("test:b", "test:a"))
        }),
        Declare(|r| r.package(package("test:a", "test:missing"))),
        Declare(|r| {
            r.package(Package {
                requires: vec!["engine:arbitrary_shaders".into()],
                ..package("test:a", "bloxgloom:core")
            })
        }),
        Declare(|r| {
            r.tag(Tag {
                key: "test:a".into(),
                kind: TagKind::Item,
                members: vec![TagMember::Tag("test:b".into())],
            })?;
            r.tag(Tag {
                key: "test:b".into(),
                kind: TagKind::Item,
                members: vec![TagMember::Tag("test:a".into())],
            })
        }),
        Declare(|r| {
            r.tag(Tag {
                key: "test:a".into(),
                kind: TagKind::Item,
                members: vec![TagMember::Definition("test:missing".into())],
            })
        }),
        Declare(|r| {
            r.texture(Texture {
                key: "test:bad".into(),
                png: vec![1, 2, 3].into(),
                stitch_edges: false,
                stitch_vertical: false,
                alpha_cutout: false,
                emission_strength: 0.0,
                foliage: Default::default(),
            })
        }),
        Declare(|r| {
            r.cube_block(bloxgloom_host_api::CubeBlock {
                key: "test:cube".into(),
                name: "CUBE".into(),
                texture: "test:missing".into(),
            })
        }),
    ];
    let catalog = Catalog::builtins();
    let before = catalog.fingerprint();
    for extension in invalid {
        assert!(crate::server::catalog_with_extension(catalog.clone(), &extension).is_err());
        assert_eq!(catalog.fingerprint(), before);
    }
}

#[test]
fn component_schema_package_and_tag_changes_are_compatibility_failures() {
    let catalog = fixture();
    let manifest = ContentManifest::from_catalog(&catalog);
    let mut changed = catalog.clone();
    changed.item_components.insert(
        CHIP.into(),
        Components::Opaque {
            version: 2,
            fingerprint: 0xc012,
            max_bytes: 16,
            required: false,
        },
    );
    assert!(manifest.resolve_catalog(&changed).is_err());
    let mut changed = manifest.clone();
    changed
        .entries
        .iter_mut()
        .find(|e| e.kind == b'P')
        .unwrap()
        .schema_fingerprint ^= 1;
    assert!(changed.resolve_catalog(&catalog).is_err());
    let mut changed = manifest;
    changed
        .entries
        .iter_mut()
        .find(|e| e.kind == b'U')
        .unwrap()
        .schema_fingerprint ^= 1;
    assert!(changed.resolve_catalog(&catalog).is_err());
}

#[test]
fn external_narrow_plant_uses_registered_selection_in_production_raycast() {
    let catalog = fixture();
    let reed = catalog.state_by_key(REED).unwrap();
    let sample = |x, y, z| {
        Some(if [x, y, z] == [0, 0, 0] {
            reed
        } else {
            world::AIR
        })
    };
    let cast = |x| {
        crate::raycast::raycast_with_catalog(
            glam::vec3(x, 0.5, -1.0),
            glam::Vec3::Z,
            3.0,
            sample,
            &catalog,
        )
    };
    assert!(cast(0.3).is_none());
    assert_eq!(cast(0.5).unwrap().block_id, reed);
    assert_eq!(catalog.plant_selection_margin(world::TALL_GRASS), 0.35);
    assert_eq!(catalog.plant_selection_margin(world::FERN), 0.22);
}

struct BlockExtension(Block);
impl Extension for BlockExtension {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), Error> {
        r.block(self.0.clone())
    }
}
fn switch_block() -> Block {
    Block {
        acoustics: None,
        key: "test:switch".into(),
        name: "SWITCH".into(),
        swatch: [0.5; 4],
        textures: FaceTextures::uniform("bloxgloom:stone"),
        geometry: Geometry::Cube,
        material: Material::Opaque,
        solid: true,
        replaceable: false,
        supports_plant: false,
        flammable: false,
        emission: 0,
        sky_attenuation: 0,
        reflectance: [128; 3],
        properties: vec![Property {
            name: "mode".into(),
            values: vec!["on".into(), "off".into()],
        }],
        states: vec![
            BlockState {
                properties: vec![("mode".into(), "on".into())],
                textures: None,
                emission: Some(12),
            },
            BlockState {
                properties: vec![("mode".into(), "off".into())],
                textures: None,
                emission: None,
            },
        ],
    }
}

#[test]
fn legal_state_sets_are_canonical_and_invalid_or_undeclared_states_fail_closed() {
    let source = switch_block();
    let a =
        crate::server::catalog_with_extension(Catalog::builtins(), &BlockExtension(source.clone()))
            .unwrap();
    let mut reordered = source.clone();
    reordered.states.reverse();
    reordered.properties[0].values.reverse();
    let b = crate::server::catalog_with_extension(Catalog::builtins(), &BlockExtension(reordered))
        .unwrap();
    assert_eq!(a.fingerprint(), b.fingerprint());
    let mut sparse = source.clone();
    sparse.states.pop();
    let sparse =
        crate::server::catalog_with_extension(Catalog::builtins(), &BlockExtension(sparse))
            .unwrap();
    let on = sparse.state_by_key("test:switch[mode=on]").unwrap();
    assert!(sparse.state_with_property(on, "mode", "off").is_none());
    let bad: [fn(&mut Block); 7] = [
        |b| b.states.push(b.states[0].clone()),
        |b| b.states[0].properties[0].1 = "unknown".into(),
        |b| b.states[0].properties.clear(),
        |b| b.states[0].emission = Some(16),
        |b| b.geometry = Geometry::CrossedPlant,
        |b| b.swatch[0] = f32::NAN,
        |b| b.textures.top = "test:missing".into(),
    ];
    for mutate in bad {
        let mut invalid = source.clone();
        mutate(&mut invalid);
        assert!(
            crate::server::catalog_with_extension(Catalog::builtins(), &BlockExtension(invalid))
                .is_err()
        );
    }
}

#[test]
fn required_components_and_registration_capacity_are_enforced_at_public_boundary() {
    let extension = Declare(|r| {
        r.item(Item {
            key: "test:required".into(),
            name: "REQUIRED".into(),
            swatch: [0.5; 4],
            texture: "bloxgloom:stick".into(),
            placeable: None,
            sprite: true,
            drop_size: bloxgloom_host_api::content::DropSize::Normal,
            drop_animation: Default::default(),
            drop_policy: Default::default(),
            components: Components::Opaque {
                version: 3,
                fingerprint: 10,
                max_bytes: 2,
                required: true,
            },
        })
    });
    let catalog = crate::server::catalog_with_extension(Catalog::builtins(), &extension).unwrap();
    let item = catalog
        .items()
        .find(|i| i.key == "test:required")
        .unwrap()
        .id;
    assert!(!Stack::new(item, 1).valid_in(&catalog));
    assert!(
        Stack::with_components(item, 1, 3, vec![1, 2])
            .unwrap()
            .valid_in(&catalog)
    );
    let too_many = Declare(|r| {
        for i in 0..4097 {
            r.tag(Tag {
                key: format!("test:tag_{i}"),
                kind: TagKind::Item,
                members: vec![],
            })?;
        }
        Ok(())
    });
    let error = crate::server::catalog_with_extension(Catalog::builtins(), &too_many).unwrap_err();
    assert!(error.to_string().contains("4096 declarations"));
    let required_placement = Declare(|r| {
        r.item(Item {
            key: "test:required_placement".into(),
            name: "INVALID PLACEMENT".into(),
            swatch: [0.5; 4],
            texture: "bloxgloom:stone".into(),
            placeable: Some("bloxgloom:stone".into()),
            sprite: false,
            drop_size: bloxgloom_host_api::content::DropSize::Normal,
            drop_animation: Default::default(),
            drop_policy: Default::default(),
            components: Components::Opaque {
                version: 1,
                fingerprint: 1,
                max_bytes: 2,
                required: true,
            },
        })
    });
    assert!(
        crate::server::catalog_with_extension(Catalog::builtins(), &required_placement).is_err()
    );
}

struct Emitter(Texture);
impl Extension for Emitter {
    fn register(&self, r: &mut dyn Registrar) -> Result<(), Error> {
        r.texture(self.0.clone())?;
        let mut block = switch_block();
        block.textures = FaceTextures::uniform(&self.0.key);
        r.block(block)
    }
}
#[test]
fn surface_emission_is_registered_bounded_and_part_of_material_compatibility() {
    let source = fixture();
    let png = source
        .textures()
        .iter()
        .find(|t| t.key == bloxgloom_lifecycle_fixture::content::TEXTURE)
        .unwrap()
        .png
        .clone();
    let mut texture = Texture {
        key: "test:emitter".into(),
        png,
        stitch_edges: false,
        stitch_vertical: false,
        alpha_cutout: false,
        emission_strength: 1.25,
        foliage: Default::default(),
    };
    let a = crate::server::catalog_with_extension(Catalog::builtins(), &Emitter(texture.clone()))
        .unwrap();
    texture.emission_strength = 2.5;
    let b = crate::server::catalog_with_extension(Catalog::builtins(), &Emitter(texture.clone()))
        .unwrap();
    assert!(
        ContentManifest::from_catalog(&a)
            .resolve_catalog(&b)
            .is_err()
    );
    for invalid in [f32::NAN, f32::INFINITY, -1.0, 16.1] {
        texture.emission_strength = invalid;
        assert!(
            crate::server::catalog_with_extension(Catalog::builtins(), &Emitter(texture.clone()))
                .is_err()
        );
    }
}
