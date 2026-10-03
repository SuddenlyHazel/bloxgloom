mod budget;
use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bloxgloom-composition-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
    fn package(&self, name: &str, dependencies: &str, startup: &str) {
        let dir = self.0.join(name);
        fs::create_dir(&dir).unwrap();
        fs::create_dir(dir.join("server")).unwrap();
        fs::create_dir_all(dir.join("assets/textures")).unwrap();
        write_pixel(dir.join("assets/textures/pixel.png"));
        fs::write(dir.join("package.txt"), format!("format 2\npackage {name}\nversion 1.0.0\nentry main\nrequires bloxgloom:content/v1\nrequires bloxgloom:owner_systems/v1\nrequires bloxgloom:generation/v1\nmodule server main server/main.luau\nmodule server callback server/callback.luau\nmodule server terrain server/terrain.luau\nasset texture pixel assets/textures/pixel.png\n{dependencies}\n")).unwrap();
        fs::write(
            dir.join("server/main.luau"),
            format!("return function(h) {startup} end"),
        )
        .unwrap();
        fs::write(
            dir.join("server/callback.luau"),
            "local calls=0; return function(c) calls+=1; return c.data .. tostring(calls), 1 end",
        )
        .unwrap();
        fs::write(
            dir.join("server/terrain.luau"),
            "return function(c) c.set_block(0,0,0,'bloxgloom:stone') end",
        )
        .unwrap();
    }
    fn discover(&self) -> std::io::Result<Declarations> {
        Declarations::discover(&self.0)
    }
    fn error(&self) -> String {
        match self.discover() {
            Ok(_) => panic!("expected failure"),
            Err(e) => e.to_string(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn system(key: &str, after: &str, data: &str) -> String {
    let owner = key.split_once(':').unwrap().0;
    format!(
        "h.register_system{{key='{key}', schema=1, revision=1, module='{owner}:callback', max_state_bytes=64, max_jobs_per_tick=1, after={{{after}}}, seeds={{{{x=0,y=0,z=0,data='{data}'}}}}}}"
    )
}

#[test]
fn multiple_shared_callbacks_keep_keyed_owner_state_and_canonical_order() {
    let fixture = Fixture::new();
    fixture.package("farm", "", &format!("{}; {}; h.register_generator('farm:z',1,'farm:terrain'); h.register_generator('farm:a',1,'farm:terrain')", system("farm:growth", "'farm:irrigation'", "G"), system("farm:irrigation", "", "I")));
    let declarations = fixture.discover().unwrap();
    assert_eq!(
        declarations
            .systems
            .iter()
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>(),
        ["farm:growth", "farm:irrigation"]
    );
    assert_eq!(
        declarations
            .generation
            .iter()
            .map(|s| s.key.as_str())
            .collect::<Vec<_>>(),
        ["farm:a", "farm:z"]
    );
    let mut states = Vec::new();
    for system in &declarations.systems {
        let context = bloxgloom_host_api::system::Context {
            environment: None,
            tags: None,
            owner: system.seeds[0].owner,
            revision: 0,
            tick: 1,
            data: &system.seeds[0].data,
            world: None,
        };
        let plan = system.behavior.plan(&context).unwrap();
        assert_eq!(
            system.behavior.plan(&context).unwrap().data,
            plan.data,
            "retry must reset module locals"
        );
        states.push(plan.data);
    }
    assert_eq!(states, [b"G1".to_vec(), b"I1".to_vec()]);
}

#[test]
fn duplicate_keys_and_caught_declaration_errors_reject_all_startup() {
    for startup in [format!("{}; pcall(function() {} end)", system("farm:a", "", "A"), system("farm:a", "", "B")), "h.register_generator('farm:a',1,'farm:terrain'); pcall(function() h.register_generator('farm:a',1,'farm:terrain') end)".into()] {
        let fixture=Fixture::new(); fixture.package("farm", "", &startup);
        assert!(fixture.error().contains("duplicate"));
    }
}

#[test]
fn phase_edges_require_registered_direct_dependencies_and_report_cycles() {
    let allowed = Fixture::new();
    allowed.package("water", "", &system("water:flow", "", "W"));
    allowed.package(
        "farm",
        "dependency water 1.0.0",
        &system("farm:growth", "'water:flow'", "G"),
    );
    allowed.discover().unwrap();
    let forbidden = Fixture::new();
    forbidden.package("water", "", &system("water:flow", "", "W"));
    forbidden.package("farm", "", &system("farm:growth", "'water:flow'", "G"));
    assert!(
        forbidden
            .error()
            .contains("explicitly declared direct dependency")
    );
    let missing = Fixture::new();
    missing.package("farm", "", &system("farm:a", "'farm:missing'", "A"));
    assert!(missing.error().contains("farm:a after farm:missing"));
    let cycle = Fixture::new();
    cycle.package(
        "farm",
        "",
        &format!(
            "{}; {}",
            system("farm:a", "'farm:b'", "A"),
            system("farm:b", "'farm:a'", "B")
        ),
    );
    assert!(cycle.error().contains("farm:a -> farm:b -> farm:a"));
}

#[test]
fn package_system_and_generator_admission_remain_bounded() {
    for (startup, resource) in [
        (
            (0..9)
                .map(|n| system(&format!("farm:s{n}"), "", "S"))
                .collect::<Vec<_>>()
                .join(";"),
            "systems/package",
        ),
        (
            (0..9)
                .map(|n| format!("h.register_generator('farm:g{n}',1,'farm:terrain')"))
                .collect::<Vec<_>>()
                .join(";"),
            "generators/package",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.package("farm", "", &startup);
        assert!(fixture.error().contains(resource));
    }
}

#[test]
fn startup_call_order_does_not_change_assigned_content_ids() {
    let mut catalogs = Vec::new();
    for names in [["z", "a"], ["a", "z"]] {
        let fixture = Fixture::new();
        let startup = names.iter().map(|name| format!("h.register_block('farm:{name}','Crop','farm:pixel'); h.register_item('farm:seed_{name}','Seed','bloxgloom:stone')")).collect::<Vec<_>>().join(";");
        fixture.package(
            "farm",
            "",
            &format!("h.register_texture('farm:pixel','pixel'); {startup}"),
        );
        let declarations = fixture.discover().unwrap();
        catalogs.push(
            crate::server::catalog_with_extension(
                crate::content::Catalog::builtins(),
                &declarations,
            )
            .unwrap(),
        );
    }
    for key in ["farm:a", "farm:z"] {
        assert_eq!(catalogs[0].block_by_key(key), catalogs[1].block_by_key(key));
        assert_eq!(catalogs[0].state_by_key(key), catalogs[1].state_by_key(key));
    }
    for key in ["farm:a", "farm:z", "farm:seed_a", "farm:seed_z"] {
        assert_eq!(catalogs[0].item_by_key(key), catalogs[1].item_by_key(key));
    }
}

fn content_capacity_fixture(extra: &str) -> Fixture {
    let fixture = Fixture::new();
    fixture.package("farm", "", &format!(
        "for i=1,256 do h.register_texture('farm:texture_'..i,'pixel'); h.register_block('farm:block_'..i,'Crop','farm:texture_'..i); h.register_item('farm:seed_'..i,'Seed','farm:texture_'..i) end; {extra}"
    ));
    fixture
}

#[test]
fn content_capacity_admits_full_block_item_texture_targets() {
    let fixture = content_capacity_fixture("");
    let declarations = fixture.discover().unwrap();
    assert_eq!(declarations.blocks.len(), 256);
    assert_eq!(
        declarations.items.len(),
        512,
        "block items share the total item allowance"
    );
    assert_eq!(declarations.textures.len(), 256);
    let catalog =
        crate::server::catalog_with_extension(crate::content::Catalog::builtins(), &declarations)
            .unwrap();
    assert!(catalog.block_by_key("farm:block_256").is_some());
    assert!(catalog.item_by_key("farm:block_256").is_some());
    assert!(catalog.item_by_key("farm:seed_256").is_some());
}

#[test]
fn content_capacity_max_plus_one_errors_survive_pcall_with_key_and_usage() {
    for (call, key, resource, attempted, maximum) in [
        (
            "h.register_block('farm:block_257','Crop','farm:texture_1')",
            "farm:block_257",
            "blocks/package",
            257,
            256,
        ),
        (
            "h.register_item('farm:seed_257','Seed','farm:texture_1')",
            "farm:seed_257",
            "items/package",
            513,
            512,
        ),
        (
            "h.register_texture('farm:texture_257','pixel')",
            "farm:texture_257",
            "textures/package",
            257,
            256,
        ),
    ] {
        let fixture = content_capacity_fixture(&format!("pcall(function() {call} end)"));
        let error = fixture.error();
        for expected in [
            "farm@1.0.0:main",
            key,
            resource,
            &format!("attempted {attempted}"),
            &format!("maximum {maximum}"),
        ] {
            assert!(error.contains(expected), "missing {expected:?} in {error}");
        }
    }
}

#[test]
fn block_auto_items_cannot_bypass_total_item_capacity() {
    let fixture = Fixture::new();
    fixture.package("farm", "", "for i=1,512 do h.register_item('farm:seed_'..i,'Seed','bloxgloom:stone') end; pcall(function() h.register_block('farm:crop','Crop','bloxgloom:stone') end)");
    let error = fixture.error();
    assert!(error.contains("register_block farm:crop"), "{error}");
    assert!(
        error.contains("items/package: attempted 513; maximum 512"),
        "{error}"
    );
}

fn system_installation_fixture(count: usize) -> Fixture {
    let fixture = Fixture::new();
    for package in 0..count.div_ceil(8) {
        let name = format!("p{package:02}");
        let startup = (0..(count - package * 8).min(8))
            .map(|n| system(&format!("{name}:s{n}"), "", "S"))
            .collect::<Vec<_>>()
            .join(";");
        fixture.package(&name, "", &startup);
    }
    fixture
}

#[test]
fn native_owner_capacity_counts_preinstalled_entries_and_rejects_atomically() {
    let fixture = system_installation_fixture(127);
    let declarations = fixture.discover().unwrap();
    let mut catalog = crate::content::Catalog::builtins();
    let mut existing = declarations.systems[0].clone();
    existing.key = "bloxgloom:preinstalled_owner".into();
    catalog.register_owner_system(existing).unwrap();
    crate::server::lifecycle::Registration::install(&declarations, &mut catalog).unwrap();
    assert_eq!(catalog.owner_systems().count(), 128);
    let extra = Fixture::new();
    extra.package(
        "extra",
        "",
        &format!(
            "h.register_texture('extra:pixel','pixel'); h.register_block('extra:unpublished','Crop','extra:pixel'); {}",
            system("extra:owner", "", "S")
        ),
    );
    let extra = extra.discover().unwrap();
    let error = match crate::server::lifecycle::Registration::install(&extra, &mut catalog) {
        Ok(_) => panic!("129 owner systems must fail"),
        Err(error) => error.to_string(),
    };
    assert!(
        error.contains("owner system") || error.contains("systems/installation"),
        "{error}"
    );
    assert_eq!(catalog.owner_systems().count(), 128);
    assert!(catalog.block_by_key("extra:unpublished").is_none());
}

fn generator_installation_fixture(count: usize) -> Fixture {
    let fixture = Fixture::new();
    for package in 0..count.div_ceil(8) {
        let name = format!("p{package:02}");
        let startup = (0..(count - package * 8).min(8))
            .map(|n| format!("h.register_generator('{name}:g{n}',1,'{name}:terrain')"))
            .collect::<Vec<_>>()
            .join(";");
        fixture.package(&name, "", &startup);
    }
    fixture
}

struct NativeGenerator(bloxgloom_host_api::generation::Registration);
impl Extension for NativeGenerator {
    fn register(&self, registrar: &mut dyn Registrar) -> Result<(), RegistrationError> {
        registrar.generation_contributor(self.0.clone())
    }
}

#[test]
fn native_generation_capacity_counts_preinstalled_contributors() {
    let fixture = generator_installation_fixture(255);
    let declarations = fixture.discover().unwrap();
    let mut existing = declarations.generation[0].clone();
    existing.key = "bloxgloom:preinstalled_terrain".into();
    let startup = crate::server::ServerStartup::new(Arc::new(crate::content::Catalog::builtins()))
        .with_extension(&NativeGenerator(existing.clone()))
        .unwrap()
        .with_extension(&declarations)
        .unwrap();
    assert_eq!(startup.generation().len(), 256);
    existing.key = "bloxgloom:overflow_terrain".into();
    let error = match startup.with_extension(&NativeGenerator(existing)) {
        Ok(_) => panic!("257 contributors must fail"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("generators/installation"), "{error}");
    let oversized = generator_installation_fixture(257);
    let declarations = oversized.discover().unwrap();
    let mut catalog = crate::content::Catalog::builtins();
    assert!(crate::server::lifecycle::Registration::install(&declarations, &mut catalog).is_err());
}

#[test]
fn caught_startup_execution_limit_cannot_publish_and_worker_recovers() {
    let fixture = Fixture::new();
    fixture.package("farm", "", "h.register_item('farm:unpublished','Seed','bloxgloom:stone'); pcall(function() while true do end end)");
    let snapshot = Arc::new(PackageSnapshot::discover(&fixture.0).unwrap());
    let worker = ScriptWorker::spawn(super::super::Limits {
        max_interrupts: 4,
        ..super::super::Limits::startup()
    })
    .unwrap();
    let output = worker.submit(
        Program::Package {
            snapshot,
            entry: "farm:main".into(),
            invocation: Invocation::Startup,
        },
        ScriptInput { tick: 0, seed: 0 },
    );
    assert!(matches!(
        output,
        Err(super::super::ScriptError {
            failure: super::super::ScriptFailure::InstructionLimit,
            ..
        })
    ));
    assert_eq!(
        worker
            .execute(
                super::super::SourceModule {
                    id: "healthy:callback".into(),
                    source: "return function(_) return 17 end".into()
                },
                ScriptInput { tick: 0, seed: 0 }
            )
            .unwrap(),
        17
    );
}

#[test]
fn repeated_texture_bindings_cannot_multiply_unbounded_host_png_storage() {
    let fixture = content_capacity_fixture("");
    let directory = fixture.0.join("farm");
    let path = directory.join("assets/textures/pixel.png");
    let mut png = fs::read(&path).unwrap();
    png.resize(2 * 1024 * 1024, 0);
    fs::write(path, png).unwrap();
    fs::write(directory.join("server/main.luau"), "return function(h) for i=1,31 do h.register_texture('farm:texture_'..i,'pixel') end; pcall(function() h.register_texture('farm:texture_32','pixel') end) end").unwrap();
    let error = fixture.error();
    for expected in [
        "farm@1.0.0:main",
        "register_texture farm:texture_32",
        "estimated declaration bytes/package",
        "attempted 67117056",
        "maximum 67108864",
    ] {
        assert!(error.contains(expected), "missing {expected:?} in {error}");
    }
}

fn write_pixel(path: PathBuf) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[120, 180, 60, 255].repeat(256))
        .unwrap();
}
