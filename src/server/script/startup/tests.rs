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
        fs::write(dir.join("package.txt"), format!("format 1\npackage {name}\nversion 1.0.0\nentry main\nrequires bloxgloom:content/v1\nrequires bloxgloom:owner_systems/v1\nrequires bloxgloom:generation/v1\nmodule main main.luau\nmodule callback callback.luau\nmodule terrain terrain.luau\n{dependencies}\n")).unwrap();
        fs::write(
            dir.join("main.luau"),
            format!("return function(h) {startup} end"),
        )
        .unwrap();
        fs::write(
            dir.join("callback.luau"),
            "local calls=0; return function(c) calls+=1; return c.data .. tostring(calls), 1 end",
        )
        .unwrap();
        fs::write(
            dir.join("terrain.luau"),
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
        let startup = names.iter().map(|name| format!("h.register_block('farm:{name}','Crop','bloxgloom:stone'); h.register_item('farm:seed_{name}','Seed','bloxgloom:stone')")).collect::<Vec<_>>().join(";");
        fixture.package("farm", "", &startup);
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
