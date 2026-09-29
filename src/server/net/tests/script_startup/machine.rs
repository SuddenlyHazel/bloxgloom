//! Luau process registration, host work planning, client screen and save identity.
use super::*;
use bloxgloom_host_api::machine::{Context, Work};

const REGISTER: &str = "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:press','Press','demo:tile'); h.register_machine{entity='demo:press_machine',block='demo:press',module='demo:tick',schema=1,revision=1,interval=20,title='STONE PRESS',hint='STONE TO GRAVEL',recipe={key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3},fuel={item='bloxgloom:stick',pulses=30}}; h.register_entity('demo:marker',1,1,0,nil) end";

fn package(fixture: &Fixture, source: &str) {
    fixture.package("demo", "requires bloxgloom:content/v1\nrequires bloxgloom:machines/v1\nrequires bloxgloom:inventory_screens/v1\nrequires bloxgloom:actions/v1\nmodule tick tick.luau", source);
    let dir = fixture.0.join("packages/demo");
    let manifest = dir.join("package.txt");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("format 1", "format 2")
        .replace(
            "module main main.luau",
            "module server main server/main.luau",
        )
        .replace(
            "module tick tick.luau",
            "module server tick server/tick.luau",
        );
    std::fs::write(
        manifest,
        format!("{text}asset texture tile assets/textures/tile.png\n"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("server")).unwrap();
    std::fs::rename(dir.join("main.luau"), dir.join("server/main.luau")).unwrap();
    std::fs::create_dir_all(dir.join("assets/textures")).unwrap();
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/material-packages/jade/assets/textures/jade.png"
        ),
        dir.join("assets/textures/tile.png"),
    )
    .unwrap();
    std::fs::write(dir.join("server/tick.luau"), "return function(c) assert(c.fuel == 0); assert(c.slots[1] == nil); return 'step',20,true end").unwrap();
}

#[test]
fn luau_machine_negotiates_plans_and_restarts() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    package(&fixture, REGISTER);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    let screen = catalog.inventory_screen(id).unwrap();
    assert_eq!(screen.hint, "STONE TO GRAVEL");
    assert_eq!(
        screen
            .groups
            .iter()
            .map(|g| g.label.as_str())
            .collect::<Vec<_>>(),
        ["FUEL", "INPUT", "OUTPUT"]
    );
    assert!(!screen.groups[2].insert);
    assert_eq!(machine.process.as_ref().unwrap().recipes[0].output_count, 2);
    let plan = machine
        .behavior
        .plan(&Context {
            tick: 100,
            due: 100,
            slots: &[None, None, None],
            data: b"",
            fuel: 0,
            progress: 0,
        })
        .unwrap();
    assert_eq!(plan.data, b"step");
    assert_eq!(plan.next_tick, 120);
    assert!(matches!(plan.work.as_slice(), [Work::Process]));
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x20")
    );
    let fingerprint = catalog.fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x542).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:press_machine").unwrap();
        assert_eq!(client.inventory_screen(id).unwrap().hint, "STONE TO GRAVEL");
        assert_eq!(
            client
                .machine(id)
                .unwrap()
                .process
                .as_ref()
                .unwrap()
                .recipes[0]
                .output_count,
            2
        );
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        REGISTER.replace("output_count=2", "output_count=3"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_machine_rejects_missing_capability_and_caught_invalid_recipe() {
    let missing = Fixture::new();
    package(&missing, REGISTER);
    let manifest = missing.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace("requires bloxgloom:machines/v1\n", "");
    std::fs::write(manifest, text).unwrap();
    assert!(missing.open().is_err());
    assert!(!missing.0.join("save/content.map").exists());

    let invalid = Fixture::new();
    package(
        &invalid,
        &REGISTER
            .replace("output_count=2", "output_count=129")
            .replace(
                "h.register_machine{",
                "pcall(function() h.register_machine{",
            )
            .replace(
                "pulses=30}}; h.register_entity",
                "pulses=30}} end); h.register_entity",
            ),
    );
    assert!(invalid.open().is_err());
    assert!(!invalid.0.join("save/content.map").exists());
}
