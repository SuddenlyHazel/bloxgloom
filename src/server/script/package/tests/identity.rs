use super::*;

fn saved_contracts(snapshot: &PackageSnapshot) -> [u64; 8] {
    [
        snapshot.gameplay_version("app:main", 2),
        snapshot.weather_observer_version("app:main", 2),
        snapshot.system_schema("app:main", 3, 2),
        snapshot.creature_schema("app:main", 3, 2),
        snapshot.machine_schema("app:main", 3, 2),
        snapshot.entity_schema("app:entity", 3, 10, 5, Some(20)),
        snapshot.entity_schema("app:entity", 3, 10, 5, None),
        snapshot.anchored_schema("app:main", 3),
    ]
}

fn packages() -> Fixture {
    let fixture = Fixture::new();
    fixture.package(
        "app",
        "module main main.luau\ndependency helper 1.0.0",
        &[("main.luau", "return function() return 1 end")],
    );
    fixture.package(
        "helper",
        "module main main.luau\ndependency leaf 1.0.0",
        &[("main.luau", "return 1")],
    );
    fixture.package(
        "leaf",
        "module main main.luau",
        &[("main.luau", "return 2")],
    );
    fixture.package(
        "other",
        "module main main.luau",
        &[("main.luau", "return 3")],
    );
    fixture
}

#[test]
fn package_saved_contracts_allow_behavior_edits_but_fence_explicit_layouts() {
    let fixture = packages();
    let original = fixture.snapshot();
    for package in ["app", "helper", "leaf", "other"] {
        fs::write(
            fixture.0.join(package).join("main.luau"),
            "return function() return 99 end",
        )
        .unwrap();
    }
    let changed = fixture.snapshot();
    assert_eq!(saved_contracts(&original), saved_contracts(&changed));
    let manifest = fixture.0.join("app/package.txt");
    fs::write(
        &manifest,
        format!(
            "{}module extracted extracted.luau\n",
            fs::read_to_string(&manifest).unwrap()
        ),
    )
    .unwrap();
    fs::write(fixture.0.join("app/extracted.luau"), "return 99").unwrap();
    assert_eq!(
        saved_contracts(&original),
        saved_contracts(&fixture.snapshot())
    );
    assert_ne!(
        original.gameplay_version("app:main", 2),
        changed.gameplay_version("app:main", 3)
    );
    assert_ne!(
        original.system_schema("app:main", 3, 2),
        changed.system_schema("app:main", 4, 2)
    );
    assert_ne!(
        original.entity_schema("app:entity", 3, 10, 5, Some(20)),
        changed.entity_schema("app:entity", 3, 11, 5, Some(20))
    );
    assert_ne!(
        original.entity_schema("app:entity", 3, 10, 5, Some(20)),
        changed.entity_schema("app:entity", 3, 10, 6, Some(20))
    );
    assert_ne!(
        original.entity_schema("app:entity", 3, 10, 5, Some(20)),
        changed.entity_schema("app:entity", 3, 10, 5, Some(21))
    );
    assert_ne!(
        original.gameplay_version("app:main", 2),
        changed.gameplay_version("helper:main", 2)
    );
}

#[test]
fn package_saved_contracts_track_declared_versions_capabilities_and_dependency_closure() {
    let fixture = packages();
    let original = fixture.snapshot();
    assert_eq!(
        original
            .dependency_closure("app:main")
            .into_iter()
            .collect::<Vec<_>>(),
        ["app", "helper", "leaf"]
    );
    let unrelated = fixture.0.join("other/package.txt");
    let text = fs::read_to_string(&unrelated).unwrap();
    fs::write(&unrelated, text.replace("version 1.0.0", "version 2.0.0")).unwrap();
    assert_eq!(
        saved_contracts(&original),
        saved_contracts(&fixture.snapshot())
    );
    fixture.package(
        "new_visuals",
        "module main main.luau",
        &[("main.luau", "return 0")],
    );
    assert_eq!(
        saved_contracts(&original),
        saved_contracts(&fixture.snapshot())
    );

    let leaf = fixture.0.join("leaf/package.txt");
    let text = fs::read_to_string(&leaf).unwrap();
    fs::write(&leaf, format!("{text}requires bloxgloom:content/v1\n")).unwrap();
    assert_ne!(
        saved_contracts(&original),
        saved_contracts(&fixture.snapshot())
    );
    fs::write(&leaf, text.replace("version 1.0.0", "version 2.0.0")).unwrap();
    let helper = fixture.0.join("helper/package.txt");
    let text = fs::read_to_string(&helper).unwrap();
    fs::write(
        &helper,
        text.replace("dependency leaf 1.0.0", "dependency leaf 2.0.0"),
    )
    .unwrap();
    assert_ne!(
        saved_contracts(&original),
        saved_contracts(&fixture.snapshot())
    );
}

#[test]
fn package_generation_digest_protects_transitive_code_but_excludes_client_art() {
    let fixture = packages();
    fs::create_dir_all(fixture.0.join("app/server")).unwrap();
    fs::create_dir_all(fixture.0.join("app/client")).unwrap();
    fs::write(
        fixture.0.join("app/server/main.luau"),
        "return function() end",
    )
    .unwrap();
    fs::write(fixture.0.join("app/client/ui.luau"), "return 1").unwrap();
    fs::write(fixture.0.join("app/package.txt"), "format 2\npackage app\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nmodule client ui client/ui.luau\ndependency helper 1.0.0\n").unwrap();
    let original = fixture.snapshot();
    original.mark_generation_module("app:main");
    let identity = original.generation_source_identity("app:main");
    fs::write(fixture.0.join("app/client/ui.luau"), "return 99").unwrap();
    fs::write(fixture.0.join("other/main.luau"), "return 99").unwrap();
    let visual_edit = fixture.snapshot();
    assert_eq!(identity, visual_edit.generation_source_identity("app:main"));
    assert_ne!(
        original.client_bundle().cache_key(),
        visual_edit.client_bundle().cache_key()
    );
    assert!(original.replacement(&fixture.0).is_ok());
    fs::write(fixture.0.join("leaf/main.luau"), "return 99").unwrap();
    let dependency_edit = fixture.snapshot();
    assert_ne!(
        identity,
        dependency_edit.generation_source_identity("app:main")
    );
    assert_eq!(
        saved_contracts(&original),
        saved_contracts(&dependency_edit)
    );

    // Live preflight and restart identity protect the same source closure.
    assert!(
        original
            .replacement(&fixture.0)
            .err()
            .unwrap()
            .to_string()
            .contains("generation")
    );
}
