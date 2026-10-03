use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-reload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("demo/server")).unwrap();
        std::fs::create_dir_all(root.join("demo/client")).unwrap();
        std::fs::write(root.join("demo/package.txt"), "format 2\npackage demo\nversion 1.0.0\nentry main\nmodule server main server/main.luau\nmodule server action server/action.luau\nmodule client client_startup client/ui.luau\nrequires bloxgloom:actions/v1\nrequires bloxgloom:owner_systems/v1\n").unwrap();
        std::fs::write(root.join("demo/server/main.luau"), "return function(h) h.register_system { key='demo:clock', schema=1, revision=1, module='demo:action', max_state_bytes=64, max_jobs_per_tick=1, read_world=false, seeds={{x=0,y=5,z=0,data='initial'}} } end").unwrap();
        std::fs::write(
            root.join("demo/server/action.luau"),
            "return function(c) return c.data, 100000 end",
        )
        .unwrap();
        std::fs::write(
            root.join("demo/client/ui.luau"),
            "return function() return 'old-client' end",
        )
        .unwrap();
        Self(std::fs::canonicalize(root).unwrap())
    }

    fn startup(&self) -> ServerStartup {
        ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&self.0)
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn package_reload_changes_client_artifact_but_preserves_frozen_catalog_and_rejects_seeds() {
    let fixture = Fixture::new();
    let startup = fixture.startup();
    let development = startup.development.as_ref().unwrap();
    std::fs::write(
        fixture.0.join("demo/client/ui.luau"),
        "return function() return 'new-client' end",
    )
    .unwrap();
    let prepared = development.prepare(startup.catalog()).unwrap();
    assert_ne!(
        prepared.bundle.cache_key(),
        startup.client_bundle.as_ref().unwrap().cache_key()
    );
    assert_eq!(
        development.base.catalog().fingerprint(),
        Catalog::builtins().fingerprint()
    );
    assert!(
        prepared
            .bundle
            .bytes()
            .windows(10)
            .any(|w| w == b"new-client")
    );
    std::fs::write(
        fixture.0.join("demo/client/ui.luau"),
        "return function() error('client-preflight') end",
    )
    .unwrap();
    let error = development.prepare(startup.catalog()).err().unwrap();
    assert!(error.to_string().contains("client-preflight"), "{error}");
    std::fs::write(
        fixture.0.join("demo/client/ui.luau"),
        "return function() end",
    )
    .unwrap();
    // Source-only revisions remain compatible for the running process.
    std::fs::write(
        fixture.0.join("demo/server/action.luau"),
        "return function(c) return c.data, 100001 end",
    )
    .unwrap();
    assert!(development.prepare(startup.catalog()).is_ok());
    let path = fixture.0.join("demo/server/main.luau");
    let original = std::fs::read_to_string(&path).unwrap();
    for changed in [
        original.replace("'initial'", "'different'"),
        original.replace("schema=1", "schema=2"),
    ] {
        std::fs::write(&path, changed).unwrap();
        let error = development.prepare(startup.catalog()).err().unwrap();
        assert!(error.to_string().contains("restart required"), "{error}");
    }
}

#[test]
fn package_reload_protects_generator_packages_and_manifest_identity() {
    let fixture = Fixture::new();
    let manifest = fixture.0.join("demo/package.txt");
    std::fs::write(
        &manifest,
        std::fs::read_to_string(&manifest).unwrap() + "requires bloxgloom:generation/v1\n",
    )
    .unwrap();
    std::fs::write(
        fixture.0.join("demo/server/main.luau"),
        "return function(h) h.register_generator('demo:terrain', 1, 'demo:action') end",
    )
    .unwrap();
    let startup = fixture.startup();
    let development = startup.development.as_ref().unwrap();
    std::fs::write(
        fixture.0.join("demo/server/action.luau"),
        "return function(_) end",
    )
    .unwrap();
    let error = development.prepare(startup.catalog()).err().unwrap();
    assert!(error.to_string().contains("generation package"), "{error}");
    std::fs::write(
        &manifest,
        std::fs::read_to_string(&manifest)
            .unwrap()
            .replace("version 1.0.0", "version 1.0.1"),
    )
    .unwrap();
    let error = development.prepare(startup.catalog()).err().unwrap();
    assert!(error.to_string().contains("manifest changed"), "{error}");
}
