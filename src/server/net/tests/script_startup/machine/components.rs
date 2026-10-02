//! Authored process predicates, exact component bytes and inert V39 identity.
use super::*;
use crate::server::entities::machine::MachinePayload;
use bloxgloom_host_api::machine::{ComponentMatch, ComponentOutput};

fn source(input: &str, output: &str) -> String {
    let items = "h.register_item('demo:raw','Raw','demo:tile',{components={version=1,fingerprint_lo=42,fingerprint_hi=7,max_bytes=4,required=true}}); h.register_item('demo:made','Made','demo:tile',{components={version=1,fingerprint_lo=42,fingerprint_hi=7,max_bytes=4,required=true}}); ";
    REGISTER
        .replace("h.register_block(", &format!("{items}h.register_block("))
        .replace("input='bloxgloom:stone'", "input='demo:raw'")
        .replace("output='bloxgloom:gravel'", "output='demo:made'")
        .replace(
            "pulses=3}",
            &format!("pulses=3,input_components={input},output_components={output}}}"),
        )
}

#[test]
fn luau_machine_component_recipe_negotiates_exact_predicate_and_preservation() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = source("{version=1,bytes=string.char(0,255)}", "'preserve_input'");
    package(&fixture, &register);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    let recipe = &machine.process.as_ref().unwrap().recipes[0];
    assert!(
        matches!(&recipe.input_components,ComponentMatch::Exact(v) if v.version==1 && v.bytes==[0,255])
    );
    assert_eq!(recipe.output_components, ComponentOutput::PreserveInput);
    assert!(machine.filters[1].components && machine.filters[2].components);
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x27")
    );
    let fingerprint = catalog.fingerprint();
    super::super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x54a).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:press_machine").unwrap();
        let machine = client.machine(id).unwrap();
        let recipe = &machine.process.as_ref().unwrap().recipes[0];
        assert_eq!(
            recipe.input_components,
            ComponentMatch::Exact(bloxgloom_host_api::machine::ComponentValue {
                version: 1,
                bytes: vec![0, 255]
            })
        );
        assert_eq!(recipe.output_components, ComponentOutput::PreserveInput);
        assert!(machine.filters[1].components && machine.filters[2].components);
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        register.replace("string.char(0,255)", "string.char(1,255)"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_machine_component_options_reject_invalid_constants_before_save() {
    for input in ["{version=0,bytes='x'}", "{version=1,bytes=''}", "'wrong'"] {
        let fixture = Fixture::new();
        package(&fixture, &source(input, "'preserve_input'"));
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/content.map").exists());
    }
}

#[test]
fn luau_machine_present_input_exact_output_and_component_fuel_roundtrip() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = source("'present'", "{version=1,bytes='z'}")
        .replace(
            "h.register_block(",
            "h.register_item('demo:fuel','Fuel','demo:tile',{components={version=1,fingerprint_lo=42,fingerprint_hi=7,max_bytes=4,required=true}}); h.register_block(",
        )
        .replace(
            "fuel={item='bloxgloom:stick',pulses=30}",
            "fuel={item='demo:fuel',pulses=30,components={version=1,bytes='f'}}",
        );
    package(&fixture, &register);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let machine = catalog
        .machine(catalog.entity_type_id_by_key("demo:press_machine").unwrap())
        .unwrap();
    let process = machine.process.as_ref().unwrap();
    assert_eq!(process.recipes[0].input_components, ComponentMatch::Present);
    assert_eq!(
        process.recipes[0].output_components,
        ComponentOutput::Exact(bloxgloom_host_api::machine::ComponentValue {
            version: 1,
            bytes: b"z".to_vec()
        })
    );
    assert!(matches!(&process.fuels[0].components,ComponentMatch::Exact(v) if v.bytes==b"f"));
    assert!(machine.filters.iter().all(|filter| filter.components));
    let fingerprint = catalog.fingerprint();
    super::super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x54b).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let machine = client
            .machine(client.entity_type_id_by_key("demo:press_machine").unwrap())
            .unwrap();
        let process = machine.process.as_ref().unwrap();
        assert_eq!(process.recipes[0].input_components, ComponentMatch::Present);
        assert_eq!(
            process.recipes[0].output_components,
            ComponentOutput::Exact(bloxgloom_host_api::machine::ComponentValue {
                version: 1,
                bytes: b"z".to_vec()
            })
        );
        assert!(matches!(&process.fuels[0].components,ComponentMatch::Exact(v) if v.bytes==b"f"));
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
}

#[test]
fn luau_component_machine_processes_exact_stack_over_listener_and_recovers() {
    process_over_listener(false);
}

#[test]
fn luau_dynamic_transformation_computes_components_over_listener_and_recovers() {
    process_over_listener(true);
}

fn process_over_listener(dynamic: bool) {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = source("{version=1,bytes=string.char(0,255)}", "'preserve_input'")
        .replace(",fuel={item='bloxgloom:stick',pulses=30}", "")
        .replace(
            "h.register_entity(",
            "h.register_tag('demo:inputs','item',{'demo:raw'}); h.register_entity(",
        );
    package(&fixture, &register);
    std::fs::write(
        fixture.0.join("packages/demo/server/tick.luau"),
        if dynamic {
            "return function(c) local s=c.slots[1]; if not s then return c.data,1,false end; assert(c.world_time().cycle_ms>0); assert(c.weather().kind); assert(c.tag_contains('item','demo:inputs',s.item)); local members=c.tag_members('item','demo:inputs',0,8); assert(members[1]==s.item); assert(not pcall(function() s.components.version=2 end)); local n=string.byte(s.components.bytes,2); return c.data,1,{{kind='transform',inputs={{slot=1,count=1}},outputs={{slot=2,item='demo:made',count=n==255 and 3 or 1,components={version=1,bytes=string.char(n-1, string.byte(s.components.bytes,1))}}}}} end"
        } else {
            "return function(c) return c.data,1,true end"
        },
    )
    .unwrap();
    const PROFILE: u128 = 0xF039;
    let mut state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let block = catalog.state_by_key("demo:press").unwrap();
    let machine_item = catalog.item_by_key("demo:press").unwrap();
    let raw = catalog.item_by_key("demo:raw").unwrap();
    let made = catalog.item_by_key("demo:made").unwrap();
    let produced = if dynamic { 3 } else { 2 };
    state.spawn_anchor = [0.5, 79.0, 0.5];
    for x in -2..=2 {
        for z in -2..=3 {
            for y in 78..=83 {
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
    let mut inventory = crate::inventory::Inventory::default();
    inventory.slots[0] = Some(Stack::new(machine_item, 1));
    inventory.slots[1] = Some(Stack::with_components(raw, 1, 1, vec![0, 255]).unwrap());
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    super::super::gameplay::serve(state, |address| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_nodelay(true).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "luau-component-machine".into(),
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
        protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint }).unwrap();
        let mut client =
            InventoryProbe::new(catalog.clone(), fixture.0.join("component-machine-probe"));
        let mut session = false;
        while !session || !client.ready(crate::world::world_to_chunk(0, 80, 2).0) {
            let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
            session |= matches!(message, ServerMessage::ActionSession { .. });
            client.accept(message);
        }
        let action_id = client.next_id();
        super::super::super::extension_lifecycle::send(
            &mut peer,
            &mut client,
            ClientMessage::Edit {
                action_id,
                x: 0,
                y: 80,
                z: 2,
                block,
                slot: 0,
            },
            &catalog,
        );
        super::super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
            c.anchored([0, 80, 2]).is_some() && c.player_count(0) == 0
        });
        client.open([0, 80, 2], block);
        let request = client.transfer(true, 1, 0, false);
        super::super::super::extension_lifecycle::send(
            &mut peer,
            &mut client,
            request.clone(),
            &catalog,
        );
        super::super::super::extension_lifecycle::send(&mut peer, &mut client, request, &catalog);
        super::super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
            c.view().unwrap().slots[1]
                .as_ref()
                .is_some_and(|stack| stack.item == made && stack.count == produced)
                && c.player_count(1) == 0
        });
        client.close();
    });
    let state = fixture.open().unwrap();
    let id = state
        .entities
        .anchored_at(crate::server::entities::CellCoord::new(0, 80, 2))
        .unwrap();
    let snapshot = state.entities.snapshot(id).unwrap();
    let payload = snapshot
        .private_payload
        .downcast_ref::<MachinePayload>()
        .unwrap();
    assert!(payload.slots[0].is_none());
    assert_eq!(
        payload.slots[1],
        Some(
            Stack::with_components(
                made,
                produced,
                1,
                if dynamic { vec![254, 0] } else { vec![0, 255] }
            )
            .unwrap()
        )
    );
    assert!(state.inventory_store.load(PROFILE).unwrap().slots[1].is_none());
}

#[test]
fn luau_transformations_capture_owned_components_and_replay_deterministically() {
    let fixture = Fixture::new();
    package(&fixture, &source("'present'", "'preserve_input'"));
    std::fs::write(
        fixture.0.join("packages/demo/server/tick.luau"),
        "local attempts=0; return function(c) attempts+=1; local s=c.slots[2]; return c.data,20,{{kind='transform',inputs={{slot=2,count=1}},outputs={{slot=3,item='demo:made',count=attempts,components={version=s.components.version,bytes=s.components.bytes..'x'}}}}} end",
    ).unwrap();
    let state = fixture.open().unwrap();
    let catalog = state.world.catalog();
    let machine = catalog
        .machine(catalog.entity_type_id_by_key("demo:press_machine").unwrap())
        .unwrap();
    let slots = [
        None,
        Some(bloxgloom_host_api::machine::Slot {
            item: "demo:raw",
            count: 3,
            has_components: true,
            stack_key: 1,
            components: Some(bloxgloom_host_api::machine::ComponentValue {
                version: 1,
                bytes: vec![0, 255],
            }),
        }),
        None,
    ];
    let context = Context {
        id: 19,
        tick: 20,
        due: 20,
        slots: &slots,
        data: b"",
        fuel: 0,
        progress: 0,
        environment: None,
        tags: None,
    };
    for _ in 0..2 {
        let plan = machine.behavior.plan(&context).unwrap();
        let [Work::Transform(work)] = plan.work.as_slice() else {
            panic!("expected transformation")
        };
        assert_eq!(work.inputs[0].expected.count, 3);
        assert_eq!(
            work.inputs[0].expected.components.as_ref().unwrap().bytes,
            [0, 255]
        );
        assert_eq!(
            work.outputs[0].stack.count, 1,
            "retry cannot retain module mutations"
        );
        assert_eq!(
            work.outputs[0].stack.components.as_ref().unwrap().bytes,
            [0, 255, b'x']
        );
    }
}
