//! Fuel-driven authored block states use one host-owned footprint and V40 identity.
use super::*;

fn source() -> String {
    REGISTER
        .replace(
            "h.register_block('demo:press','Press','demo:tile')",
            "h.register_block('demo:press','Press','demo:tile',{properties={lit={'off','on'}},states={{lit='off'},{lit='on',emission=12}}})",
        )
        .replace(
            "title='STONE PRESS'",
            "active_state='demo:press[lit=on]',title='STONE PRESS'",
        )
}

#[test]
fn luau_machine_active_state_negotiates_and_preserves_save_identity() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = source();
    package(&fixture, &register);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    assert_eq!(machine.variants[0].idle[0].state, "demo:press[lit=off]");
    assert_eq!(machine.variants[0].active[0].state, "demo:press[lit=on]");
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x28")
    );
    let fingerprint = catalog.fingerprint();
    super::super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x54c).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let machine = client
            .machine(client.entity_type_id_by_key("demo:press_machine").unwrap())
            .unwrap();
        assert_eq!(machine.variants[0].idle[0].state, "demo:press[lit=off]");
        assert_eq!(machine.variants[0].active[0].state, "demo:press[lit=on]");
    });
    assert_eq!(
        fixture.open().unwrap().world.catalog().fingerprint(),
        fingerprint
    );
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        register.replace("emission=12", "emission=13"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_machine_active_state_rejects_foreign_and_unpowered_states() {
    let fixture = Fixture::new();
    package(
        &fixture,
        &source().replace("demo:press[lit=on]", "bloxgloom:stone"),
    );
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save/content.map").exists());

    let fixture = Fixture::new();
    package(
        &fixture,
        &source().replace(",fuel={item='bloxgloom:stick',pulses=30}", ""),
    );
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save/content.map").exists());
}

#[test]
fn luau_machine_fuel_switches_authored_state_over_listener_and_recovers() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = source()
        .replace("interval=20", "interval=1")
        .replace("pulses=3}", "pulses=150}")
        .replace("pulses=30}", "pulses=200}");
    package(&fixture, &register);
    std::fs::write(
        fixture.0.join("packages/demo/server/tick.luau"),
        "return function(c) return c.data,1,true end",
    )
    .unwrap();
    const PROFILE: u128 = 0xF040;
    let mut state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let idle = catalog.state_by_key("demo:press[lit=off]").unwrap();
    let active = catalog.state_by_key("demo:press[lit=on]").unwrap();
    let item = catalog.item_by_key("demo:press").unwrap();
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
    inventory.slots[0] = Some(Stack::new(item, 1));
    inventory.slots[1] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:stone").unwrap(),
        1,
    ));
    inventory.slots[2] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:stick").unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    super::super::gameplay::serve(state, |address| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_nodelay(true).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "luau-active-machine".into(),
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
            InventoryProbe::new(catalog.clone(), fixture.0.join("active-machine-probe"));
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
                block: idle,
                slot: 0,
            },
            &catalog,
        );
        super::super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
            c.anchored([0, 80, 2]).is_some()
                && c.block_state([0, 80, 2]) == Some(idle)
                && c.player_count(0) == 0
        });
        client.open([0, 80, 2], idle);
        for (player, container) in [(1, 1), (2, 0)] {
            let request = client.transfer(true, player, container, false);
            super::super::super::extension_lifecycle::send(
                &mut peer,
                &mut client,
                request,
                &catalog,
            );
            super::super::super::extension_lifecycle::until(
                &mut peer,
                &mut client,
                &catalog,
                |c| c.player_count(player as usize) == 0,
            );
        }
        super::super::super::extension_lifecycle::until(&mut peer, &mut client, &catalog, |c| {
            c.block_state([0, 80, 2]) == Some(active)
                && c.player_count(1) == 0
                && c.player_count(2) == 0
        });
        client.close();
    });
    let mut state = fixture.open().unwrap();
    assert_eq!(state.world.get_block(0, 80, 2).unwrap(), active);
    let id = state
        .entities
        .anchored_at(crate::server::entities::CellCoord::new(0, 80, 2))
        .unwrap();
    let snapshot = state.entities.snapshot(id).unwrap();
    let payload = snapshot
        .private_payload
        .downcast_ref::<crate::server::entities::machine::MachinePayload>()
        .unwrap();
    assert!(payload.fuel > 0);
    assert!(state.inventory_store.load(PROFILE).unwrap().slots[1].is_none());
    assert!(state.inventory_store.load(PROFILE).unwrap().slots[2].is_none());
}
