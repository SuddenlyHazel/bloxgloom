#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::server::script::{Limits, ScriptInput, ScriptWorker};

mod capacity;
mod client;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-packages-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        // macOS temp roots commonly use /var -> /private/var. Discovery itself
        // deliberately rejects root-ancestor links as well as package links.
        Self(fs::canonicalize(root).unwrap())
    }

    fn package(&self, name: &str, declarations: &str, modules: &[(&str, &str)]) {
        let directory = self.0.join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("package.txt"),
            format!("format 1\npackage {name}\nversion 1.0.0\nentry main\n{declarations}\n"),
        )
        .unwrap();
        for (path, source) in modules {
            let path = directory.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, source).unwrap();
        }
    }

    fn snapshot(&self) -> Arc<PackageSnapshot> {
        Arc::new(PackageSnapshot::discover(&self.0).unwrap())
    }

    fn error(&self) -> ScriptError {
        PackageSnapshot::discover(&self.0)
            .err()
            .expect("invalid package set")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn input() -> ScriptInput {
    ScriptInput { tick: 8, seed: 13 }
}

#[test]
fn worker_uses_declared_imports_frozen_sources_and_fresh_module_state() {
    let fixture = Fixture::new();
    fixture.package(
        "math",
        "module main code/main.luau",
        &[(
            "code/main.luau",
            "counter = (counter or 0) + 1; return { count = counter, value = 7 }",
        )],
    );
    fixture.package("app", "module unused unused.luau\nmodule main main.luau\ndependency math 1.0.0", &[
        ("unused.luau", "this is deliberately not valid Luau"),
        ("main.luau", "local a = import('math:main'); local b = import('math:main'); assert(a == b); assert(counter == nil); a.count += 1; return function(input) return a.value * 10 + a.count + input.tick end")]);
    let snapshot = fixture.snapshot();
    assert_eq!(
        snapshot
            .packages
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["app", "math"]
    );
    fs::remove_dir_all(fixture.0.join("math")).unwrap();
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    for _ in 0..2 {
        assert_eq!(
            worker
                .execute_package(Arc::clone(&snapshot), "app", input())
                .unwrap(),
            80
        );
        // Cached Lua exports/callbacks must not form an ownership cycle that
        // keeps the request VM or its snapshot alive after the reply.
        assert_eq!(Arc::strong_count(&snapshot), 1);
    }
}

#[test]
fn imports_are_lexical_direct_dependencies_not_transitive_or_paths() {
    let fixture = Fixture::new();
    fixture.package(
        "secret",
        "module main main.luau",
        &[("main.luau", "return 7")],
    );
    fixture.package(
        "bridge",
        "dependency secret 1.0.0\nmodule main main.luau",
        &[(
            "main.luau",
            "return function() return import('secret:main') end",
        )],
    );
    fixture.package(
        "app",
        "dependency bridge 1.0.0\nmodule main main.luau",
        &[(
            "main.luau",
            r#"
        return function(_)
            assert(not pcall(import, 'secret:main'))
            assert(not pcall(import, '../secret/main.luau'))
            assert(not pcall(import, '/etc/passwd'))
            assert(not pcall(import, 'bridge:undeclared'))
            assert(not pcall(import, string.rep('x', 130)))
            assert(require == nil and os.clock == nil and os.time == nil and os.date == nil and io == nil and getfenv == nil and setfenv == nil)
            return import('bridge:main')()
        end
    "#,
        )],
    );
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    assert_eq!(
        worker
            .execute_package(fixture.snapshot(), "app", input())
            .unwrap(),
        7
    );
}

#[test]
fn dependency_versions_and_manifest_declarations_are_strict() {
    let fixture = Fixture::new();
    fixture.package(
        "app",
        "dependency missing 1.0.0\nmodule main main.luau",
        &[("main.luau", "return 1")],
    );
    assert_eq!(fixture.error().module, "app@1.0.0");
    fixture.package(
        "missing",
        "module main main.luau",
        &[("main.luau", "return 1")],
    );
    let manifest = fixture.0.join("missing/package.txt");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("1.0.0", "2.0.0"),
    )
    .unwrap();
    assert!(fixture.error().to_string().contains("missing@1.0.0"));
    for extra in [
        "package other",
        "version 01.0.0",
        "entry absent",
        "format 2",
        "module main other.luau",
        "dependency x 1.0",
        "dependency app 1.0.0",
        "module x ../escape.luau",
        "module x /absolute.luau",
        "module x a//b.luau",
        "module x a\\b.luau",
        "unknown ignored",
    ] {
        let manifest = format!(
            "format 1\npackage app\nversion 1.0.0\nentry main\nmodule main main.luau\n{extra}"
        );
        assert!(Manifest::parse("app", &manifest).is_err(), "{extra}");
    }
    for version in ["01.0.0", "1.0", "1.0.0.0", "1.0.0-beta", "4294967296.0.0"] {
        let manifest =
            format!("format 1\npackage app\nversion {version}\nentry main\nmodule main main.luau");
        assert!(Manifest::parse("app", &manifest).is_err(), "{version}");
    }
}

#[test]
fn import_failures_name_package_version_and_module_and_do_not_poison_worker() {
    let fixture = Fixture::new();
    fixture.package(
        "bad",
        "module main main.luau",
        &[("main.luau", "return function(")],
    );
    fixture.package(
        "app",
        "dependency bad 1.0.0\nmodule main main.luau",
        &[(
            "main.luau",
            "local x = import('bad:main'); return function(_) return x end",
        )],
    );
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    let broken = fixture.snapshot();
    let error = worker
        .execute_package(Arc::clone(&broken), "app", input())
        .unwrap_err();
    assert_eq!(error.module, "bad@1.0.0:main");
    assert!(matches!(error.failure, ScriptFailure::Lua(_)));
    fs::write(fixture.0.join("bad/main.luau"), "return 9").unwrap();
    assert_eq!(
        worker
            .execute_package(fixture.snapshot(), "app", input())
            .unwrap(),
        9
    );
    assert_eq!(
        worker
            .execute_package(broken, "app", input())
            .unwrap_err()
            .module,
        "bad@1.0.0:main"
    );
}

#[test]
fn failed_modules_are_not_reinitialized_when_caught() {
    let fixture = Fixture::new();
    fixture.package("app", "module main main.luau\nmodule broken broken.luau\nmodule shared shared.luau", &[
        ("shared.luau", "return { attempts = 0 }"),
        ("broken.luau", "local state = import('app:shared'); state.attempts += 1; error('failed init')"),
        ("main.luau", "assert(not pcall(import, 'app:broken')); assert(not pcall(import, 'app:broken')); return function(_) return import('app:shared').attempts end")]);
    let worker = ScriptWorker::spawn(Limits::default()).unwrap();
    let snapshot = fixture.snapshot();
    for _ in 0..2 {
        assert_eq!(
            worker
                .execute_package(Arc::clone(&snapshot), "app", input())
                .unwrap(),
            1
        );
    }
}

#[test]
fn imported_source_limits_and_exported_function_errors_keep_source_identity() {
    let fixture = Fixture::new();
    fixture.package(
        "lib",
        "module main main.luau",
        &[("main.luau", &format!("--{}\nreturn 1", "x".repeat(100)))],
    );
    fixture.package(
        "app",
        "dependency lib 1.0.0\nmodule main main.luau",
        &[(
            "main.luau",
            "return function(_) return import('lib:main')() end",
        )],
    );
    let worker = ScriptWorker::spawn(Limits {
        max_source_bytes: 100,
        ..Limits::default()
    })
    .unwrap();
    let error = worker
        .execute_package(fixture.snapshot(), "app", input())
        .unwrap_err();
    assert_eq!(error.module, "lib@1.0.0:main");
    assert_eq!(error.failure, ScriptFailure::SourceTooLarge);
    fs::write(
        fixture.0.join("lib/main.luau"),
        "return function() error('export failed') end",
    )
    .unwrap();
    let error = worker
        .execute_package(fixture.snapshot(), "app", input())
        .unwrap_err();
    assert_eq!(error.module, "app@1.0.0:main");
    assert!(
        matches!(error.failure, ScriptFailure::Lua(message) if message.contains("lib@1.0.0:main") && message.contains("export failed"))
    );
}

#[test]
fn cycles_depth_and_shared_execution_budget_are_bounded() {
    let fixture = Fixture::new();
    fixture.package(
        "app",
        "module main main.luau\nmodule other other.luau",
        &[
            ("main.luau", "return import('app:other')"),
            ("other.luau", "return import('app:main')"),
        ],
    );
    let worker = ScriptWorker::spawn(Limits {
        max_interrupts: 100,
        max_wall_time: std::time::Duration::from_secs(2),
        ..Limits::default()
    })
    .unwrap();
    let error = worker
        .execute_package(fixture.snapshot(), "app", input())
        .unwrap_err();
    assert_eq!(error.module, "app@1.0.0:main");
    assert!(error.to_string().contains("cyclic import"));
    fs::write(fixture.0.join("app/other.luau"), "while true do end").unwrap();
    let error = worker
        .execute_package(fixture.snapshot(), "app", input())
        .unwrap_err();
    assert_eq!(error.module, "app@1.0.0:other");
    assert_eq!(error.failure, ScriptFailure::InstructionLimit);

    let mut declarations = "module main main.luau\n".to_owned();
    fs::write(fixture.0.join("app/main.luau"), "return import('app:m0')").unwrap();
    for index in 0..33 {
        declarations.push_str(&format!("module m{index} m{index}.luau\n"));
        fs::write(
            fixture.0.join(format!("app/m{index}.luau")),
            format!("return import('app:m{}')", index + 1),
        )
        .unwrap();
    }
    fixture.package("app", &declarations, &[]);
    let worker = ScriptWorker::spawn(Limits {
        max_wall_time: std::time::Duration::from_secs(2),
        ..Limits::default()
    })
    .unwrap();
    let error = worker
        .execute_package(fixture.snapshot(), "app", input())
        .unwrap_err();
    assert!(
        error.to_string().contains("import depth limit exceeded"),
        "{error}"
    );
}

#[test]
fn discovery_bounds_directory_count_source_bytes_and_total_bytes() {
    let fixture = Fixture::new();
    for index in 0..=MAX_PACKAGES {
        fs::create_dir(fixture.0.join(format!("p{index}"))).unwrap();
    }
    assert!(
        fixture
            .error()
            .to_string()
            .contains("too many package directories")
    );
    let fixture = Fixture::new();
    fixture.package(
        "app",
        "module main main.luau",
        &[("main.luau", &"x".repeat(MAX_SOURCE_BYTES + 1))],
    );
    assert_eq!(fixture.error().module, "app@1.0.0:main");
    let fixture = Fixture::new();
    for name in ["a", "b"] {
        let mut declarations = String::new();
        fixture.package(name, "", &[]);
        for index in 0..crate::server::script::capacity::MAX_MODULES_PER_PACKAGE {
            let module = if index == 0 {
                "main".to_owned()
            } else {
                format!("m{index}")
            };
            declarations.push_str(&format!("module {module} {module}.luau\n"));
            fs::write(
                fixture.0.join(name).join(format!("{module}.luau")),
                vec![b'x'; MAX_SOURCE_BYTES],
            )
            .unwrap();
        }
        fixture.package(name, &declarations, &[]);
    }
    assert!(fixture.error().to_string().contains("bounded regular file"));
}

#[test]
fn discovery_rejects_symlinks_at_every_path_level_and_special_files() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    fixture.package(
        "app",
        "module main code/main.luau",
        &[("code/main.luau", "return 1")],
    );
    let outside = Fixture::new();
    fs::write(outside.0.join("source.luau"), "return 999").unwrap();
    let source = fixture.0.join("app/code/main.luau");
    fs::remove_file(&source).unwrap();
    symlink(outside.0.join("source.luau"), &source).unwrap();
    assert_eq!(fixture.error().module, "app@1.0.0:main");
    fs::remove_file(&source).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fixture.error().module, "app@1.0.0:main");
    fs::remove_dir_all(fixture.0.join("app/code")).unwrap();
    symlink(&outside.0, fixture.0.join("app/code")).unwrap();
    assert_eq!(fixture.error().module, "app@1.0.0:main");
    fs::remove_file(fixture.0.join("app/package.txt")).unwrap();
    symlink(
        outside.0.join("source.luau"),
        fixture.0.join("app/package.txt"),
    )
    .unwrap();
    assert_eq!(fixture.error().module, "app");
    fs::remove_dir_all(fixture.0.join("app")).unwrap();
    symlink(&outside.0, fixture.0.join("app")).unwrap();
    assert_eq!(fixture.error().module, "app");
    assert!(PackageSnapshot::discover(&fixture.0.join("app")).is_err());
    assert!(PackageSnapshot::discover(&fixture.0.join("app/child")).is_err());
    assert!(PackageSnapshot::discover(&fixture.0.join("..")).is_err());
}
