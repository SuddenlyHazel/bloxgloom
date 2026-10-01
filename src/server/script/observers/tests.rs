use super::*;
use bloxgloom_host_api::gameplay::{CommittedBlock, CommittedEntity, Entity};

#[test]
fn committed_observer_copies_exact_readonly_public_handles_without_writer() {
    let event = Committed {
        blocks: vec![CommittedBlock {
            cell: [-1, 80, 2],
            state: "bloxgloom:stone".into(),
        }],
        entities: vec![
            CommittedEntity::Spawned(Entity {
                id: u64::MAX,
                entity_type: "demo:actor".into(),
                position: [1., 80., 2.],
                anchor: None,
                data: vec![0, 255],
            }),
            CommittedEntity::Removed {
                id: u64::MAX - 1,
                key: "demo:actor".into(),
            },
        ],
        inventory: Some((u128::MAX, u64::MAX)),
    };
    let source = super::super::SourceModule {
        id: "demo:observe".into(),
        source: r#"return function(e)
            assert(e.kind=='Committed' and e.blocks[1].cell[1]==-1)
            assert(e.blocks[1].state=='bloxgloom:stone')
            assert(e.entities[1].kind=='Spawned' and e.entities[2].kind=='Removed')
            assert(tostring(e.entities[1].entity)=='entity:ffffffffffffffff')
            assert(e.entities[1].data==string.char(0,255))
            assert(tostring(e.inventory.profile)=='profile:ffffffffffffffffffffffffffffffff')
            assert(e.inventory.slots==nil and e.set_block==nil and e.give==nil)
            for _,v in {e,e.blocks,e.blocks[1],e.blocks[1].cell,e.entities,e.entities[1],e.inventory} do
                assert(not pcall(function() v.bad=1 end))
            end
        end"#.into(),
    };
    super::super::run_with(
        &Program::Source(source),
        Limits::default(),
        super::super::runtime::Execution::new("Committed", events::seed(&event), "advisory"),
        |lua, entry| entry.call::<()>(events::present(lua, &event)?),
    )
    .unwrap();
}

#[test]
fn observer_entries_retain_imports_and_reset_after_failure_or_world_retirement() {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-observer-realm-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("demo")).unwrap();
    let root = fs::canonicalize(root).unwrap();
    fs::write(root.join("demo/package.txt"), "format 1\npackage demo\nversion 1.0.0\nentry main\nmodule main main.luau\nmodule cache cache.luau\n").unwrap();
    fs::write(root.join("demo/cache.luau"), "return {count=0}").unwrap();
    fs::write(root.join("demo/main.luau"), "local cache=import('demo:cache'); return function(e) cache.count+=1; assert(e.blocks[1].cell[1]==cache.count) end").unwrap();
    let snapshot = Arc::new(PackageSnapshot::discover(&root).unwrap());
    let observer = || ScriptObserver {
        snapshot: Arc::clone(&snapshot),
        module: "demo:main".into(),
        lifetime: Arc::new(()),
        realm: NEXT_REALM.fetch_add(1, Ordering::Relaxed),
    };
    let event = |n| Committed {
        blocks: vec![CommittedBlock {
            cell: [n, 0, 0],
            state: "bloxgloom:stone".into(),
        }],
        entities: vec![],
        inventory: None,
    };
    let first = observer();
    first.invoke(&event(1)).unwrap();
    first.invoke(&event(2)).unwrap();
    let second = observer();
    second.invoke(&event(1)).unwrap();
    assert!(first.invoke(&event(0)).is_err());
    first.invoke(&event(1)).unwrap();
    drop(first);
    drop(second);
    observer().invoke(&event(1)).unwrap();
    fs::remove_dir_all(root).unwrap();
}
