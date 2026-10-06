//! Source edits through real nonblocking joins and persistent world recovery.
use super::*;
use crate::world::{AIR, ChunkKey, GLOWSTONE, SAND, STONE};
use gameplay::{Peer, serve};

const PROFILE: u128 = 0x5c71;
const KEY: ChunkKey = ChunkKey { x: 1, y: 5, z: 0 };
const REGISTER: &str = r#"return function(h)
    h.register_item('demo:token', 'Token', 'bloxgloom:stone')
    h.register_entity('demo:marker',1,3,1,nil)
    h.register_action('demo:shift',1,'Shift','item','demo:token','demo:action')
    h.register_system{key='demo:clock',schema=1,revision=1,module='demo:clock',max_state_bytes=16,max_jobs_per_tick=1,read_world=true,creates_entities=true,seeds={{x=1,y=5,z=0,data='new'}}}
end"#;
const OWNER: &str = "return function(c) if c.data=='new' then c.spawn_entity('demo:marker',22,80,0,string.char(1,0,255)) end return 'saved',100000 end";

fn package(fixture: &Fixture) {
    fixture.package("demo", "", REGISTER);
    let path = fixture.0.join("packages/demo");
    std::fs::create_dir_all(path.join("server")).unwrap();
    std::fs::create_dir_all(path.join("client")).unwrap();
    std::fs::rename(path.join("main.luau"), path.join("server/main.luau")).unwrap();
    std::fs::write(path.join("package.txt"), "format 2\npackage demo\nversion 1.0.0\nentry main\ndependency helper 1.0.0\nrequires bloxgloom:content/v1\nrequires bloxgloom:actions/v1\nrequires bloxgloom:owner_systems/v1\nmodule server main server/main.luau\nmodule server action server/action.luau\nmodule server clock server/clock.luau\nmodule client view client/view.luau\n").unwrap();
    std::fs::write(path.join("server/clock.luau"), OWNER).unwrap();
    std::fs::write(
        path.join("client/view.luau"),
        "return function() return 'first client' end",
    )
    .unwrap();
    fixture.package(
        "helper",
        "module naming naming.luau",
        "return function(_) end",
    );
    behavior(fixture, 2, "bloxgloom:glowstone");
}

fn behavior(fixture: &Fixture, x: i32, block: &str) {
    std::fs::write(
        fixture.0.join("packages/helper/naming.luau"),
        format!("return '{block}'"),
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/server/action.luau"), format!("local block=import('helper:naming'); return function(c,e) assert(c.transfer(0,1,1)); local before=c.block({x},80,0).state; local after=if string.byte(e.arguments,1)==1 then before elseif before==block then 'bloxgloom:sand' else block; c.set_block({x},80,0,after) end")).unwrap();
}

fn marker(state: &State) -> (u64, u64, Vec<u8>) {
    let id = state
        .entities
        .query_mobile_aabb([22.0, 80.0, 0.0], [23.0, 81.0, 1.0])
        .unwrap()[0];
    let entity = state.entities.snapshot(id).unwrap();
    (
        id.get(),
        entity.revision,
        entity
            .private_payload
            .downcast_ref::<Vec<u8>>()
            .unwrap()
            .clone(),
    )
}

fn value(state: &State, system: &str) -> (u64, Vec<u8>) {
    state
        .system_runtime
        .owner_value::<Vec<u8>>(
            &crate::server::registry::SystemId::new(system).unwrap(),
            crate::server::parallel::OwnerKey::Chunk(KEY),
        )
        .unwrap()
}

#[test]
fn package_behavior_dependency_and_client_edits_restart_with_inventory_entities_and_deadlines() {
    let fixture = Fixture::new();
    package(&fixture);
    let mut identity = None;
    let mut entity = None;
    let mut previous_bundle = None;
    for round in 0..3 {
        if round == 1 {
            // Change executable behavior and an imported dependency together.
            behavior(&fixture, 3, "bloxgloom:stone");
            std::fs::write(
                fixture.0.join("packages/demo/server/clock.luau"),
                "return function(c) assert(c.data=='saved'); return 'updated',100000 end",
            )
            .unwrap();
        } else if round == 2 {
            // A client-only revision must change its verified artifact but
            // must leave all durable server contracts unchanged.
            std::fs::write(
                fixture.0.join("packages/demo/client/view.luau"),
                "return function() return 'second client' end",
            )
            .unwrap();
        }
        let mut state = Box::new(fixture.open().unwrap());
        state.spawn_anchor = [0.5, 80.0, 0.5];
        let catalog = state.world.catalog_arc();
        let token = catalog.item_by_key("demo:token").unwrap();
        let current = (token, catalog.fingerprint());
        if round == 0 {
            identity = Some(current);
            for (x, y, z, block) in [
                (0, 79, 0, STONE),
                (0, 80, 0, AIR),
                (0, 81, 0, AIR),
                (2, 80, 0, AIR),
                (3, 80, 0, AIR),
                (22, 80, 0, AIR),
            ] {
                state.world.edit(x, y, z, block).unwrap();
            }
            state.world.get_chunk(KEY).unwrap();
            system::commit(&mut state, "demo:clock", 1);
            entity = Some(marker(&state));
            let mut inventory = Inventory::default();
            assert_eq!(inventory.insert_with_catalog(token, 128, &catalog), 0);
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        } else {
            assert_eq!(Some(current), identity);
            assert_eq!(Some(marker(&state)), entity);
            assert_eq!(value(&state, "demo:clock"), (1, b"saved".to_vec()));
            assert!(
                system::stage(&mut state, "demo:clock", 100000)
                    .unwrap()
                    .0
                    .is_none(),
                "saved deadline ran early after source edit"
            );
        }
        let bundle = state.client_bundle.as_ref().unwrap().cache_key();
        if round == 2 {
            assert_ne!(previous_bundle, Some(bundle));
        }
        previous_bundle = Some(bundle);
        serve(state, |address| {
            let mut peer = Peer::connect(address, Arc::clone(&catalog));
            assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 128 - round);
            assert!(
                peer.inventory
                    .slots
                    .iter()
                    .flatten()
                    .all(|stack| stack.count <= 128)
            );
            assert_eq!(
                peer.inventory
                    .slots
                    .iter()
                    .flatten()
                    .map(|stack| u32::from(stack.count))
                    .sum::<u32>(),
                128
            );
            let request = peer.request(0);
            assert!(peer.send(&request).0, "restarted behavior did not execute");
            if round == 2 {
                // Repeating an action with an unchanged world cell still
                // transfers its item once and keeps the connection alive.
                let deadline = Instant::now() + Duration::from_secs(10);
                while peer.inventory.slots[0].as_ref().unwrap().count != 125 {
                    peer.read(deadline);
                }
                let repeat = peer.request(1);
                let result = peer.send(&repeat);
                assert!(result.0, "no-op action failed after restart: {}", result.1);
            }
        });
        let mut restored = fixture.open().unwrap();
        assert_eq!(
            restored
                .world
                .get_block(if round == 0 { 2 } else { 3 }, 80, 0)
                .unwrap(),
            if round == 0 {
                GLOWSTONE
            } else if round == 1 {
                STONE
            } else {
                SAND
            }
        );
        assert_eq!(Some(marker(&restored)), entity);
        assert_eq!(value(&restored, "demo:clock"), (1, b"saved".to_vec()));
    }
    let mut restored = fixture.open().unwrap();
    let inventory = restored.inventory_store.load(PROFILE).unwrap();
    assert_eq!(inventory.slots[0].as_ref().unwrap().count, 124);
    assert_eq!(
        inventory
            .slots
            .iter()
            .flatten()
            .map(|stack| u32::from(stack.count))
            .sum::<u32>(),
        128
    );
    // The same recovered deadline now invokes the edited owner behavior.
    restored.world.get_chunk(KEY).unwrap();
    system::commit(&mut restored, "demo:clock", 100001);
    assert_eq!(value(&restored, "demo:clock"), (2, b"updated".to_vec()));
    drop(restored);
    assert_eq!(
        value(&fixture.open().unwrap(), "demo:clock"),
        (2, b"updated".to_vec())
    );
}

fn saved_files(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        result: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, result)
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = std::collections::BTreeMap::new();
    walk(root, root, &mut result);
    result
}

#[test]
fn incompatible_package_contracts_and_generation_leave_saved_files_unchanged() {
    let fixture = Fixture::new();
    package(&fixture);
    fixture.reopen();
    let before = saved_files(&fixture.0.join("save"));
    for source in [
        REGISTER.replace("schema=1", "schema=2"),
        REGISTER.replace("marker',1", "marker',2"),
        REGISTER.replace("shift',1", "shift',2"),
        REGISTER.replace("'Token', 'bloxgloom:stone'", "'Token', 'bloxgloom:dirt'"),
    ] {
        std::fs::write(fixture.0.join("packages/demo/server/main.luau"), source).unwrap();
        assert!(
            fixture.open().is_err(),
            "incompatible declaration opened save"
        );
        assert_eq!(saved_files(&fixture.0.join("save")), before);
    }
    std::fs::write(fixture.0.join("packages/demo/server/main.luau"), REGISTER).unwrap();
    fixture.reopen();
    let terrain = Fixture::new();
    terrain.generator(
        "return function(h) h.register_generator('demo:terrain',1,'demo:terrain') end",
        "return function(c) c.set_block(8,0,8,'bloxgloom:stone') end",
    );
    terrain.reopen();
    let before = saved_files(&terrain.0.join("save"));
    std::fs::write(
        terrain.0.join("packages/demo/terrain.luau"),
        "return function(c) c.set_block(8,0,8,'bloxgloom:glowstone') end",
    )
    .unwrap();
    assert!(
        terrain.open().is_err(),
        "same-revision generator edit changed saved world"
    );
    assert_eq!(saved_files(&terrain.0.join("save")), before);
}
