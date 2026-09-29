//! Luau process registration, host work planning, client screen and save identity.
use super::*;
use crate::client::InventoryProbe;
use crate::inventory::Stack;
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
fn v37_combines_creature_read_policy_with_machine_descriptor() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let source = REGISTER.replace(
        "h.register_entity('demo:marker'",
        "h.register_creature{key='demo:sproutling',module='demo:critter',schema=1,revision=1,max_state_bytes=8,interval=1,reads_neighbours=true,body={half_width=0.25,height=0.7,speed=1.0},model={{min={-0.2,0.0,-0.2},max={0.2,0.7,0.2},color={0.2,0.8,0.3}}}}; h.register_entity('demo:marker'",
    );
    package(&fixture, &source);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, format!("{text}requires bloxgloom:mobile_entities/v1\nmodule server critter server/critter.luau\n")).unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/server/critter.luau"),
        "return function(c) return c.data,10,nil,nil end",
    )
    .unwrap();
    let state = Box::new(fixture.open().unwrap());
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x25")
    );
    let fingerprint = state.world.catalog().fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x546).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let machine = client
            .machine(client.entity_type_id_by_key("demo:press_machine").unwrap())
            .unwrap();
        assert_eq!(machine.process.as_ref().unwrap().recipes.len(), 1);
        let creature = client
            .mobile_entity(client.entity_type_id_by_key("demo:sproutling").unwrap())
            .unwrap();
        assert!(creature.reads_neighbours);
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
fn luau_machine_footprint_negotiates_across_seam_and_restarts() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = REGISTER.replace(
        "fuel={item='bloxgloom:stick',pulses=30}",
        "fuel={item='bloxgloom:stick',pulses=30},footprint={{0,0,0},{1,0,0}}",
    );
    package(&fixture, &register);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    let placement_state = &machine.variants[0].placement_state;
    let placed = machine.plan_place([15, 80, 0], placement_state).unwrap();
    assert_eq!(
        placed.cells.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
        [[15, 80, 0], [16, 80, 0]]
    );
    let stored = placed.cells.iter().map(|(at, _)| *at).collect::<Vec<_>>();
    assert!(
        machine
            .plan_remove([15, 80, 0], [16, 80, 0], 0, false, &stored)
            .is_ok()
    );
    assert_eq!(
        catalog.inventory_screen(id).unwrap().footprint,
        [[0, 0, 0], [1, 0, 0]]
    );
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x24")
    );
    let fingerprint = catalog.fingerprint();
    super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x546).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:press_machine").unwrap();
        assert_eq!(
            client.inventory_screen(id).unwrap().footprint,
            [[0, 0, 0], [1, 0, 0]]
        );
        assert_eq!(client.machine(id).unwrap().variants[0].idle.len(), 2);
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
}

#[test]
fn luau_machine_footprint_places_and_breaks_secondary_cell_over_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = REGISTER.replace(
        "fuel={item='bloxgloom:stick',pulses=30}",
        "fuel={item='bloxgloom:stick',pulses=30},footprint={{0,0,0},{1,0,0}}",
    );
    package(&fixture, &register);
    const PROFILE: u128 = 0xF036;
    let anchor = [-1, 80, 2];
    let secondary = [0, 80, 2];
    for restarted in [false, true] {
        let mut state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        let block = catalog.state_by_key("demo:press").unwrap();
        let item = catalog.item_by_key("demo:press").unwrap();
        state.spawn_anchor = [0.5, 79.0, 0.5];
        if !restarted {
            for x in -2..=1 {
                for z in -1..=3 {
                    for y in 78..=82 {
                        state
                            .world
                            .edit(
                                x,
                                y,
                                z,
                                if y == 78 {
                                    crate::world::STONE
                                } else {
                                    crate::world::AIR
                                },
                            )
                            .unwrap();
                    }
                }
            }
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(Stack::new(item, 1));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        }
        super::gameplay::serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_nodelay(true).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "luau-footprint".into(),
                    profile: PROFILE,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap()
            else {
                panic!("expected bundle offer");
            };
            crate::client::bundle::receive(&mut peer, identity, None).unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let mut client = InventoryProbe::new(catalog.clone(), fixture.0.join("probe-config"));
            let mut session = false;
            while !session
                || !client.ready(crate::world::world_to_chunk(-1, 80, 2).0)
                || !client.ready(crate::world::world_to_chunk(0, 80, 2).0)
            {
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                session |= matches!(message, ServerMessage::ActionSession { .. });
                client.accept(message);
            }
            if !restarted {
                let action_id = client.next_id();
                super::super::extension_lifecycle::send(
                    &mut peer,
                    &mut client,
                    ClientMessage::Edit {
                        action_id,
                        x: anchor[0],
                        y: anchor[1],
                        z: anchor[2],
                        block,
                        slot: 0,
                    },
                    &catalog,
                );
                super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(anchor).is_some() && c.player_count(0) == 0
                });
                client.open(secondary, block);
                client.close();
            } else {
                super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(anchor).is_some()
                });
                client.open(secondary, block);
                client.close();
                let action_id = client.next_id();
                let remove = ClientMessage::Edit {
                    action_id,
                    x: secondary[0],
                    y: secondary[1],
                    z: secondary[2],
                    block: crate::world::AIR,
                    slot: 0,
                };
                super::super::extension_lifecycle::send(
                    &mut peer,
                    &mut client,
                    remove.clone(),
                    &catalog,
                );
                super::super::extension_lifecycle::send(&mut peer, &mut client, remove, &catalog);
                super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(anchor).is_none()
                });
            }
        });
        let mut recovered = fixture.open().unwrap();
        assert_eq!(
            recovered
                .world
                .get_block(anchor[0], anchor[1], anchor[2])
                .unwrap(),
            if restarted { crate::world::AIR } else { block }
        );
        assert_eq!(
            recovered
                .world
                .get_block(secondary[0], secondary[1], secondary[2])
                .unwrap(),
            if restarted { crate::world::AIR } else { block }
        );
        assert_eq!(
            recovered
                .entities
                .anchored_at(crate::server::entities::CellCoord::new(
                    anchor[0], anchor[1], anchor[2]
                ))
                .is_some(),
            !restarted
        );
        if restarted {
            let held = recovered
                .inventory_store
                .load(PROFILE)
                .unwrap()
                .slots
                .iter()
                .flatten()
                .filter(|stack| stack.item == item)
                .map(|stack| stack.count)
                .sum::<u16>();
            let dropped = crate::server::drops::nearby(&recovered.entities, [-0.5, 80.5, 2.5])
                .into_iter()
                .filter_map(|view| {
                    crate::server::drops::stack(
                        &recovered.entities,
                        crate::server::entities::EntityId::new(view.id).unwrap(),
                    )
                })
                .filter(|stack| stack.item == item)
                .map(|stack| stack.count)
                .sum::<u16>();
            assert_eq!(
                held + dropped,
                1,
                "break must conserve the one placed machine item"
            );
        }
    }
}

#[test]
fn luau_machine_rejects_unanchored_or_duplicate_footprint_before_save() {
    for footprint in ["{{1,0,0}}", "{{0,0,0},{0,0,0}}", "{{0,0,0},{3,0,0}}"] {
        let fixture = Fixture::new();
        let register = REGISTER.replace(
            "fuel={item='bloxgloom:stick',pulses=30}",
            &format!("fuel={{item='bloxgloom:stick',pulses=30}},footprint={footprint}"),
        );
        package(&fixture, &register);
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/content.map").exists());
    }
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
