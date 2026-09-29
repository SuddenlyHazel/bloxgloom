//! Authored storage declarations and production lifecycle acceptance.
use super::*;
use crate::client::InventoryProbe;

#[test]
fn luau_storage_screen_negotiates_host_owned_inventory() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); h.register_storage('demo:chest','demo:jade','Jade Chest',9,3) end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\n"),
    )
    .unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let entity = catalog.entity_type_id_by_key("demo:chest").unwrap();
    assert_eq!(catalog.inventory_screen(entity).unwrap().slots, 9);
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x1d")
    );
    let fingerprint = catalog.fingerprint();
    serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x529).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        assert_eq!(
            client
                .inventory_screen(client.entity_type_id_by_key("demo:chest").unwrap())
                .unwrap()
                .columns,
            3
        );
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
}

#[test]
fn luau_storage_layout_negotiates_and_is_saved_identity() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let source = "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); h.register_storage('demo:chest','demo:jade','Jade Chest',9,3,{hint='Keeps nine stacks',groups={{label='TOOLS',count=3},{label='SUPPLIES',count=6}}}) end";
    package(&fixture, source);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\n"),
    )
    .unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let entity = catalog.entity_type_id_by_key("demo:chest").unwrap();
    let screen = catalog.inventory_screen(entity).unwrap();
    assert_eq!(screen.hint, "Keeps nine stacks");
    assert_eq!(screen.groups[1].first, 3);
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x1e")
    );
    let fingerprint = catalog.fingerprint();
    serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x52a).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let screen = client
            .inventory_screen(client.entity_type_id_by_key("demo:chest").unwrap())
            .unwrap();
        assert_eq!(screen.hint, "Keeps nine stacks");
        assert_eq!(screen.groups[1].label, "SUPPLIES");
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        source.replace("Keeps nine stacks", "Keeps tools"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_storage_footprint_negotiates_across_seam_and_restarts() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let source = "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); h.register_storage('demo:chest','demo:jade','Jade Chest',9,3,{footprint={{0,0,0},{1,0,0}},hint='Two cells'}) end";
    package(&fixture, source);
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\n"),
    )
    .unwrap();
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:chest").unwrap();
    let storage = catalog
        .storage_lifecycles
        .iter()
        .find(|storage| storage.entity == "demo:chest")
        .unwrap();
    let placed = storage
        .plan_place(bloxgloom_host_api::lifecycle::PlacementContext {
            anchor: [-1, 80, 2],
            state: &storage.anchor_state,
        })
        .unwrap();
    assert_eq!(
        placed.cells.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
        [[-1, 80, 2], [0, 80, 2]]
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
            .starts_with(b"BGCLIENT\x26")
    );
    let fingerprint = catalog.fingerprint();
    serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x547).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let id = client.entity_type_id_by_key("demo:chest").unwrap();
        assert_eq!(
            client.inventory_screen(id).unwrap().footprint,
            [[0, 0, 0], [1, 0, 0]]
        );
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        source.replace("{1,0,0}", "{0,0,1}"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_storage_rejects_bad_footprints_before_world_open() {
    for footprint in ["{{1,0,0}}", "{{0,0,0},{0,0,0}}", "{{0,0,0},{3,0,0}}"] {
        let fixture = Fixture::new();
        let source = format!(
            "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); pcall(function() h.register_storage('demo:chest','demo:jade','Jade Chest',9,3,{{footprint={footprint}}}) end) end"
        );
        package(&fixture, &source);
        let manifest = fixture.0.join("packages/demo/package.txt");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            format!(
                "{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\n"
            ),
        )
        .unwrap();
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/content.map").exists());
    }
}

#[test]
fn luau_storage_footprint_places_opens_and_breaks_from_secondary_cell() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); h.register_storage('demo:chest','demo:jade','Jade Chest',9,3,{footprint={{0,0,0},{1,0,0}}}) end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\n"),
    )
    .unwrap();
    const PROFILE: u128 = 0xF038;
    let anchor = [-1, 80, 2];
    let secondary = [0, 80, 2];
    for restarted in [false, true] {
        let mut state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        let block = catalog.state_by_key("demo:jade").unwrap();
        let item = catalog.item_by_key("demo:jade").unwrap();
        state.spawn_anchor = [0.5, 79.0, 0.5];
        if !restarted {
            for x in -2..=1 {
                for z in -1..=3 {
                    for y in 78..=82 {
                        state
                            .world
                            .edit(x, y, z, if y == 78 { STONE } else { AIR })
                            .unwrap();
                    }
                }
            }
            let mut inventory = Inventory::default();
            inventory.slots[0] = Some(crate::inventory::Stack::new(item, 1));
            state.inventory_store.save(PROFILE, &inventory).unwrap();
        }
        serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_nodelay(true).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "luau-storage-footprint".into(),
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
            let mut client = InventoryProbe::new(catalog.clone(), fixture.0.join("storage-probe"));
            let mut session = false;
            while !session
                || !client.ready(crate::world::world_to_chunk(anchor[0], anchor[1], anchor[2]).0)
                || !client
                    .ready(crate::world::world_to_chunk(secondary[0], secondary[1], secondary[2]).0)
            {
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                session |= matches!(message, ServerMessage::ActionSession { .. });
                client.accept(message);
            }
            if !restarted {
                let action_id = client.next_id();
                super::super::super::super::extension_lifecycle::send(
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
                super::super::super::super::extension_lifecycle::until(
                    &mut peer,
                    &mut client,
                    &catalog,
                    |c| c.anchored(anchor).is_some() && c.player_count(0) == 0,
                );
            } else {
                super::super::super::super::extension_lifecycle::until(
                    &mut peer,
                    &mut client,
                    &catalog,
                    |c| c.anchored(anchor).is_some(),
                );
            }
            client.open(secondary, block);
            client.close();
            if restarted {
                let action_id = client.next_id();
                let remove = ClientMessage::Edit {
                    action_id,
                    x: secondary[0],
                    y: secondary[1],
                    z: secondary[2],
                    block: AIR,
                    slot: 0,
                };
                super::super::super::super::extension_lifecycle::send(
                    &mut peer,
                    &mut client,
                    remove.clone(),
                    &catalog,
                );
                super::super::super::super::extension_lifecycle::send(
                    &mut peer,
                    &mut client,
                    remove,
                    &catalog,
                );
                super::super::super::super::extension_lifecycle::until(
                    &mut peer,
                    &mut client,
                    &catalog,
                    |c| c.anchored(anchor).is_none(),
                );
            }
        });
        let mut recovered = fixture.open().unwrap();
        for cell in [anchor, secondary] {
            assert_eq!(
                recovered
                    .world
                    .get_block(cell[0], cell[1], cell[2])
                    .unwrap(),
                if restarted { AIR } else { block }
            );
        }
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
            assert_eq!(held + dropped, 1);
        }
    }
}

#[test]
fn luau_storage_requires_declared_capabilities_before_world_open() {
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); h.register_storage('demo:chest','demo:jade','Jade Chest',9,3) end",
    );
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save/content.map").exists());
}

#[test]
fn luau_storage_rejects_caught_incomplete_group_layout() {
    let fixture = Fixture::new();
    package(
        &fixture,
        "return function(h) h.register_texture('demo:tile','tile'); h.register_block('demo:jade','Jade','demo:tile'); pcall(function() h.register_storage('demo:chest','demo:jade','Jade Chest',9,3,{groups={{label='ONLY',count=3}}}) end) end",
    );
    let manifest = fixture.0.join("packages/demo/package.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}requires bloxgloom:storage/v1\nrequires bloxgloom:inventory_screens/v1\n"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save/content.map").exists());
}
