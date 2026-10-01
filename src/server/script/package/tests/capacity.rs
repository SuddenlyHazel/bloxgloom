use super::*;
use crate::server::script::capacity::{MAX_ASSETS_PER_PACKAGE, MAX_MODULES_PER_PACKAGE};

#[test]
fn coordinated_module_capacity_accepts_target_and_rejects_local_and_global_excess() {
    let fixture = Fixture::new();
    let declarations = (0..MAX_MODULES_PER_PACKAGE)
        .map(|i| {
            let key = if i == 0 {
                "main".into()
            } else {
                format!("m{i}")
            };
            format!("module shared {key} shared/common.luau")
        })
        .collect::<Vec<_>>()
        .join("\n");
    for index in 0..(MAX_MODULES / MAX_MODULES_PER_PACKAGE) {
        client::classified(
            &fixture,
            &format!("p{index}"),
            &declarations,
            &[("shared/common.luau", "return function() return 1 end")],
        );
    }
    let snapshot = fixture.snapshot();
    let bundle = snapshot.client_bundle();
    let decoded = crate::server::script::package::client::ClientBundle::decode_verify(
        bundle.bytes(),
        bundle.cache_key(),
    )
    .unwrap();
    assert_eq!(
        decoded
            .packages()
            .values()
            .map(|p| p.sources.len())
            .sum::<usize>(),
        MAX_MODULES
    );
    fixture.package(
        "extra",
        "module main main.luau",
        &[("main.luau", "return 1")],
    );
    let error = fixture.error();
    assert!(
        error.to_string().contains("modules/installation"),
        "{error}"
    );
    fs::remove_dir_all(fixture.0.join("extra")).unwrap();
    let path = fixture.0.join("p0/package.txt");
    let mut manifest = fs::read_to_string(&path).unwrap();
    manifest.push_str("module shared extra shared/common.luau\n");
    fs::write(path, manifest).unwrap();
    let error = fixture.error();
    assert!(
        error.to_string().contains(&format!(
            "modules/package: attempted {}; maximum {MAX_MODULES_PER_PACKAGE}",
            MAX_MODULES_PER_PACKAGE + 1
        )),
        "{error}"
    );
}

#[test]
fn asset_local_admission_attributes_key_and_file() {
    let declarations = (0..=MAX_ASSETS_PER_PACKAGE)
        .map(|i| format!("asset texture a{i} assets/textures/a.png"))
        .collect::<Vec<_>>()
        .join("\n");
    let fixture = Fixture::new();
    client::classified(
        &fixture,
        "app",
        &format!("module server main server/main.luau\n{declarations}"),
        &[
            ("server/main.luau", "return function() end"),
            ("assets/textures/a.png", ""),
        ],
    );
    let error = fixture.error().to_string();
    assert!(
        error.contains("app")
            && error.contains("assets/textures/a.png")
            && error.contains("assets/package: attempted 257; maximum 256"),
        "{error}"
    );
}
