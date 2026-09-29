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

#[test]
fn luau_machine_recipe_list_negotiates_filters_and_restarts() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let multi = REGISTER.replace(
        "recipe={key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3}",
        "recipes={{key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3},{key='demo:polish',input='bloxgloom:gravel',input_count=2,output='bloxgloom:stone',output_count=1,pulses=5}}",
    );
    package(&fixture, &multi);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    let process = machine.process.as_ref().unwrap();
    assert_eq!(process.recipes.len(), 2);
    assert_eq!(process.recipes[1].key, "demo:polish");
    assert_eq!(process.recipes[1].pulses, 5);
    assert_eq!(
        machine.filters[1].items,
        ["bloxgloom:gravel", "bloxgloom:stone"]
    );
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x22")
    );
    let fingerprint = catalog.fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x543).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:press_machine").unwrap();
        let machine = client.machine(id).unwrap();
        assert_eq!(
            machine.process.as_ref().unwrap().recipes[1].key,
            "demo:polish"
        );
        assert_eq!(
            machine.filters[1].items,
            ["bloxgloom:gravel", "bloxgloom:stone"]
        );
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        multi.replace("pulses=5", "pulses=6"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_machine_recipe_list_rejects_overlapping_inputs_before_save() {
    let fixture = Fixture::new();
    let duplicate = REGISTER.replace(
        "recipe={key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3}",
        "recipes={{key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3},{key='demo:again',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=1,pulses=5}}",
    );
    package(&fixture, &duplicate);
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save/content.map").exists());
}

#[test]
fn v34_combines_recipe_lists_and_creature_options_in_one_client_catalog() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let multi = REGISTER.replace(
        "recipe={key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3}",
        "recipes={{key='demo:crush',input='bloxgloom:stone',input_count=1,output='bloxgloom:gravel',output_count=2,pulses=3},{key='demo:polish',input='bloxgloom:gravel',input_count=2,output='bloxgloom:stone',output_count=1,pulses=5}}",
    ).replace(
        "h.register_entity('demo:marker'",
        "h.register_creature{key='demo:sproutling',module='demo:critter',schema=1,revision=1,max_state_bytes=8,interval=1,body={half_width=0.25,height=0.7,speed=1.0},interaction='pat',animation={idle_bob=0.02},model={{min={-0.2,0.0,-0.2},max={0.2,0.7,0.2},color={0.2,0.8,0.3}}}}; h.register_entity('demo:marker'",
    );
    package(&fixture, &multi);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, format!("{text}requires bloxgloom:mobile_entities/v1\nmodule server critter server/critter.luau\n")).unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/server/critter.luau"),
        "return function(c) if c.event == 'interact' then return c.data end return c.data,10,nil,nil end",
    ).unwrap();
    let state = Box::new(fixture.open().unwrap());
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x22")
    );
    let fingerprint = state.world.catalog().fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x544).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let machine = client
            .machine(client.entity_type_id_by_key("demo:press_machine").unwrap())
            .unwrap();
        assert_eq!(machine.process.as_ref().unwrap().recipes.len(), 2);
        let creature = client
            .mobile_entity(client.entity_type_id_by_key("demo:sproutling").unwrap())
            .unwrap();
        assert_eq!(creature.interaction, b"pat");
        assert_eq!(creature.animation.idle_bob, 0.02);
    });
}

#[test]
fn luau_machine_ports_and_transfer_work_negotiate_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = REGISTER.replace(
        "fuel={item='bloxgloom:stick',pulses=30}",
        "fuel={item='bloxgloom:stick',pulses=30},ports={{name='feed',faces={{1,0,0}},insert={2},extract={3}}}",
    );
    package(&fixture, &register);
    std::fs::write(
        fixture.0.join("packages/demo/server/tick.luau"),
        "return function(c) return 'step',20,{{kind='transfer',offset={1,0,0},port='feed',push=true,count=2,source_slot=3},{kind='process'}} end",
    ).unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    assert_eq!(machine.read_radius, 1);
    assert!(machine.reads_neighbours);
    assert_eq!(machine.ports[0].name, "feed");
    assert_eq!(machine.ports[0].faces, [[1, 0, 0]]);
    assert_eq!(machine.ports[0].insert, [1]);
    assert_eq!(machine.ports[0].extract, [2]);
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
    assert_eq!(plan.work.len(), 2);
    assert!(
        matches!(&plan.work[0], Work::Transfer {offset, own_port, push: true, selection, ..}
        if *offset == [1,0,0] && own_port == "feed" && selection.count == 2 && selection.source_slot == Some(2))
    );
    assert!(matches!(plan.work[1], Work::Process));
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x23")
    );
    let fingerprint = catalog.fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x545).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:press_machine").unwrap();
        let machine = client.machine(id).unwrap();
        assert_eq!(machine.ports[0].extract, [2]);
        assert_eq!(machine.read_radius, 1);
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        register.replace("extract={3}", "extract={2}"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_machine_rejects_invalid_ports_and_undeclared_transfer_work() {
    let bad_port = Fixture::new();
    package(
        &bad_port,
        &REGISTER.replace(
            "fuel={item='bloxgloom:stick',pulses=30}",
            "fuel={item='bloxgloom:stick',pulses=30},ports={{name='bad',faces={{1,1,0}},extract={3}}}",
        ),
    );
    assert!(bad_port.open().is_err());
    assert!(!bad_port.0.join("save/content.map").exists());

    let bad_work = Fixture::new();
    package(
        &bad_work,
        &REGISTER.replace(
            "fuel={item='bloxgloom:stick',pulses=30}",
            "fuel={item='bloxgloom:stick',pulses=30},ports={{name='feed',faces={{1,0,0}},extract={3}}}",
        ),
    );
    std::fs::write(
        bad_work.0.join("packages/demo/server/tick.luau"),
        "return function(c) return 'bad',20,{{kind='transfer',offset={1,0,0},port='missing',push=true}} end",
    ).unwrap();
    let state = bad_work.open().unwrap();
    let id = state
        .world
        .catalog()
        .entity_type_id_by_key("demo:press_machine")
        .unwrap();
    let machine = state.world.catalog().machine(id).unwrap();
    assert!(
        machine
            .behavior
            .plan(&Context {
                tick: 100,
                due: 100,
                slots: &[None, None, None],
                data: b"",
                fuel: 0,
                progress: 0,
            })
            .is_err()
    );
}
