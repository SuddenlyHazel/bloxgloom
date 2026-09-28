use super::*;
use crate::server::script::package::client::{ClientBundle, ClientSide};

fn classified(fixture: &Fixture, name: &str, declarations: &str, files: &[(&str, &str)]) {
    fixture.package(name, declarations, files);
    let path = fixture.0.join(name).join("package.txt");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replacen("format 1", "format 2", 1);
    fs::write(path, text).unwrap();
}

#[test]
fn discovery_exports_only_classified_frozen_bytes_in_canonical_order() {
    let a = Fixture::new();
    let b = Fixture::new();
    let declarations = [
        "module server main server/main.luau",
        "module client ui client/ui.luau",
        "module shared common shared/common.luau",
        "asset texture icon assets/textures/icon.png",
        "dependency library 1.0.0",
    ];
    for (fixture, reverse) in [(&a, false), (&b, true)] {
        if reverse {
            fixture.package(
                "library",
                "module main main.luau",
                &[("main.luau", "PRIVATE library")],
            );
        }
        let lines = if reverse {
            declarations.iter().rev().copied().collect::<Vec<_>>()
        } else {
            declarations.to_vec()
        };
        classified(
            fixture,
            "app",
            &lines.join("\n"),
            &[
                ("server/main.luau", "PRIVATE server"),
                ("client/ui.luau", "return 'client'"),
                ("shared/common.luau", "return 'shared'"),
                ("assets/textures/icon.png", "opaque texture"),
                ("assets/textures/unlisted.png", "PRIVATE unlisted"),
                ("world-v9/drops.bin", "PRIVATE drops"),
                ("inventories/player.bin", "PRIVATE inventory"),
            ],
        );
        if !reverse {
            fixture.package(
                "library",
                "module main main.luau",
                &[("main.luau", "PRIVATE library")],
            );
        }
    }
    let first = a.snapshot();
    let second = b.snapshot();
    let bundle = first.client_bundle();
    assert_eq!(bundle.bytes(), second.client_bundle().bytes());
    assert_eq!(bundle.cache_key(), second.client_bundle().cache_key());
    assert!(!bundle.bytes().windows(7).any(|w| w == b"PRIVATE"));
    assert!(!bundle.bytes().windows(11).any(|w| w == b"server/main"));
    let decoded = ClientBundle::decode_verify(bundle.bytes(), bundle.cache_key()).unwrap();
    assert!(decoded.packages()["library"].sources.is_empty());
    let app = &decoded.packages()["app"];
    assert_eq!(app.sources.len(), 2);
    assert_eq!(app.sources["common"].side, ClientSide::Shared);
    assert_eq!(app.sources["ui"].side, ClientSide::Client);
    assert_eq!(app.textures["icon"], b"opaque texture");
    assert_eq!(
        bundle.cache_key().cache_name().len(),
        "client-v7-sha256-".len() + 64
    );
    fs::write(a.0.join("app/server/main.luau"), "changed private source").unwrap();
    assert_eq!(a.snapshot().client_bundle().cache_key(), bundle.cache_key());
    fs::write(a.0.join("app/client/ui.luau"), "changed client source").unwrap();
    assert_ne!(a.snapshot().client_bundle().cache_key(), bundle.cache_key());
    fs::remove_dir_all(a.0.join("app")).unwrap();
    assert_eq!(bundle.bytes(), second.client_bundle().bytes());
}

#[test]
fn classified_packages_preserve_clientless_startup_and_server_import_authority() {
    let fixture = Fixture::new();
    classified(
        &fixture,
        "app",
        "module server main server/main.luau\nmodule shared common shared/common.luau\nmodule client ui client/ui.luau",
        &[
            (
                "server/main.luau",
                "return function(_) assert(import('app:common') == 7); assert(not pcall(import, 'app:ui')); return 7 end",
            ),
            ("shared/common.luau", "return 7"),
            ("client/ui.luau", "error('must never run on the server')"),
        ],
    );
    crate::server::script::startup::Declarations::discover(&fixture.0).unwrap();
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    assert_eq!(
        worker
            .execute_package(fixture.snapshot(), "app", input())
            .unwrap(),
        7
    );
}

#[test]
fn export_paths_types_and_sides_fail_closed() {
    for declaration in [
        "module client ui server/private.luau",
        "module shared ui client/ui.luau",
        "module client ui client/../server/private.luau",
        "module client ui /client/ui.luau",
        "module client ui client/.secret.luau",
        "module client ui client/a/b/c/d/e/f/g/h.luau",
        "asset texture icon world-v9/drops.bin",
        "asset texture icon assets/textures/drops.bin",
        "asset texture icon assets/textures/../../save.png",
        "asset texture icon assets/textures/.private.png",
        "asset script icon assets/textures/script.png",
        "module main main.luau",
    ] {
        let fixture = Fixture::new();
        classified(
            &fixture,
            "app",
            &format!("module server main server/main.luau\n{declaration}"),
            &[("server/main.luau", "return function() end")],
        );
        assert!(
            matches!(fixture.error().failure, ScriptFailure::Package(_)),
            "{declaration}"
        );
    }
    let fixture = Fixture::new();
    classified(
        &fixture,
        "app",
        "module client main client/main.luau",
        &[("client/main.luau", "return 1")],
    );
    fixture.error(); // server entry cannot name a client module
}

#[test]
fn assets_use_secure_bounded_regular_file_reads() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    classified(
        &fixture,
        "app",
        "module server main server/main.luau\nasset texture icon assets/textures/icon.png",
        &[
            ("server/main.luau", "return function() end"),
            ("assets/textures/icon.png", ""),
        ],
    );
    let path = fixture.0.join("app/assets/textures/icon.png");
    fs::write(&path, vec![255; MAX_ASSET_BYTES]).unwrap();
    assert_eq!(
        fixture.snapshot().client_bundle().packages()["app"].textures["icon"].len(),
        MAX_ASSET_BYTES
    );
    fs::write(&path, vec![0; MAX_ASSET_BYTES + 1]).unwrap();
    fixture.error();
    fs::remove_file(&path).unwrap();
    symlink("../../server/main.luau", &path).unwrap();
    fixture.error();
    fs::remove_file(&path).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    fixture.error();
}

#[test]
fn asset_aggregate_bytes_and_declaration_count_are_bounded() {
    let fixture = Fixture::new();
    let mut declarations = String::from("module server main server/main.luau\n");
    for i in 0..17 {
        declarations.push_str(&format!("asset texture a{i} assets/textures/icon.png\n"));
    }
    classified(
        &fixture,
        "app",
        &declarations,
        &[
            ("server/main.luau", "return function() end"),
            ("assets/textures/icon.png", ""),
        ],
    );
    fs::write(
        fixture.0.join("app/assets/textures/icon.png"),
        vec![0; MAX_ASSET_BYTES],
    )
    .unwrap();
    fixture.error();
    for i in 17..65 {
        declarations.push_str(&format!("asset texture a{i} assets/textures/icon.png\n"));
    }
    classified(
        &fixture,
        "app",
        &declarations,
        &[("assets/textures/icon.png", "")],
    );
    fixture.error();
}

#[test]
fn asset_set_count_is_bounded_even_for_empty_files() {
    let fixture = Fixture::new();
    let mut declarations = String::from("module server main server/main.luau\n");
    for i in 0..64 {
        declarations.push_str(&format!("asset texture a{i} assets/textures/icon.png\n"));
    }
    for i in 0..4 {
        classified(
            &fixture,
            &format!("p{i}"),
            &declarations,
            &[
                ("server/main.luau", "return function() end"),
                ("assets/textures/icon.png", ""),
            ],
        );
    }
    assert_eq!(
        fixture
            .snapshot()
            .client_bundle()
            .packages()
            .values()
            .map(|p| p.textures.len())
            .sum::<usize>(),
        MAX_ASSETS
    );
    classified(
        &fixture,
        "p4",
        "module server main server/main.luau\nasset texture extra assets/textures/icon.png",
        &[
            ("server/main.luau", "return function() end"),
            ("assets/textures/icon.png", ""),
        ],
    );
    assert!(fixture.error().to_string().contains("too many assets"));
}
