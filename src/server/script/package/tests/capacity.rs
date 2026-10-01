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

#[test]
fn exact_file_byte_targets_are_admitted_and_one_more_byte_is_attributed() {
    use crate::server::script::capacity::{MAX_ASSET_BYTES, MAX_MANIFEST_BYTES, MAX_TOTAL_BYTES};
    let fixture = Fixture::new();
    let assets = (0..16)
        .map(|i| format!("asset texture a{i:02} assets/textures/a{i:02}.png"))
        .collect::<Vec<_>>()
        .join("\n");
    client::classified(
        &fixture,
        "app",
        &format!("module shared main shared/main.luau\n{assets}"),
        &[("shared/main.luau", "return function() end")],
    );
    let directory = fixture.0.join("app");
    let manifest = directory.join("package.txt");
    let mut text = fs::read_to_string(&manifest).unwrap();
    text.extend(std::iter::repeat_n('\n', MAX_MANIFEST_BYTES - text.len()));
    fs::write(&manifest, &text).unwrap();
    let source = directory.join("shared/main.luau");
    let mut code = fs::read_to_string(&source).unwrap();
    code.extend(std::iter::repeat_n(' ', MAX_SOURCE_BYTES - code.len()));
    fs::write(&source, &code).unwrap();
    fs::create_dir_all(directory.join("assets/textures")).unwrap();
    let mut png = include_bytes!("../../../../../assets/textures/blocks/stone.png").to_vec();
    png.resize(MAX_ASSET_BYTES, 0);
    for i in 0..15 {
        fs::write(directory.join(format!("assets/textures/a{i:02}.png")), &png).unwrap();
    }
    let last = directory.join("assets/textures/a15.png");
    let remaining = MAX_TOTAL_BYTES - text.len() - code.len() - 15 * png.len();
    fs::write(&last, &png[..remaining]).unwrap();
    let snapshot = fixture.snapshot();
    let bundle = snapshot.client_bundle();
    crate::server::script::package::client::ClientBundle::decode_verify(
        bundle.bytes(),
        bundle.cache_key(),
    )
    .unwrap();
    drop(snapshot);
    fs::write(&last, &png[..remaining + 1]).unwrap();
    let error = fixture.error().to_string();
    assert!(
        error.contains("a15") && error.contains("file bytes/installation"),
        "{error}"
    );
    fs::write(&last, &png[..remaining]).unwrap();
    fs::write(&manifest, format!("{text}\n")).unwrap();
    assert!(
        fixture
            .error()
            .to_string()
            .contains("manifest bytes/package")
    );
    fs::write(&manifest, text).unwrap();
    fs::write(&source, format!("{code} ")).unwrap();
    assert!(fixture.error().to_string().contains("source bytes/module"));
    fs::write(&source, code).unwrap();
    fs::write(
        directory.join("assets/textures/a00.png"),
        [png.as_slice(), &[0]].concat(),
    )
    .unwrap();
    assert!(fixture.error().to_string().contains("asset bytes/file"));
}
