//! Multiple authored placement states share a footprint and one finite machine item.
use super::*;

fn source() -> String {
    REGISTER
        .replace(
            "h.register_block('demo:press','Press','demo:tile')",
            "h.register_block('demo:press','Press','demo:tile',{properties={face={'north','south'},lit={'off','on'}},states={{face='north',lit='off'},{face='north',lit='on',emission=12},{face='south',lit='off'},{face='south',lit='on',emission=12}}})",
        )
        .replace(
            "title='STONE PRESS'",
            "footprint={{0,0,0},{1,0,0}},variants={{state='demo:press[face=north,lit=off]',active_state='demo:press[face=north,lit=on]'},{state='demo:press[face=south,lit=off]',active_state='demo:press[face=south,lit=on]'}},title='STONE PRESS'",
        )
}

#[test]
fn luau_machine_variants_negotiate_place_from_finite_inventory_and_recover() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let register = source();
    package(&fixture, &register);
    const PROFILE: u128 = 0xF041;
    let mut state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    let id = catalog.entity_type_id_by_key("demo:press_machine").unwrap();
    let machine = catalog.machine(id).unwrap();
    assert_eq!(machine.variants.len(), 2);
    let south = catalog
        .state_by_key("demo:press[face=south,lit=off]")
        .unwrap();
    let item = catalog.item_by_key("demo:press").unwrap();
    let plan = machine
        .plan_place([0, 80, 2], "demo:press[face=south,lit=off]")
        .unwrap();
    assert_eq!(plan.variant, 1);
    assert_eq!(plan.cells.len(), 2);
    assert!(
        plan.cells
            .iter()
            .all(|(_, state)| state == "demo:press[face=south,lit=off]")
    );
    assert!(
        state
            .client_bundle
            .as_ref()
            .unwrap()
            .bytes()
            .starts_with(b"BGCLIENT\x29")
    );
    let fingerprint = catalog.fingerprint();
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
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    super::super::gameplay::serve(state, |address| {
        let client = crate::client::connect_catalog_probe(&address.to_string(), 0x54d).unwrap();
        assert_eq!(client.fingerprint(), fingerprint);
        let client_machine = client
            .machine(client.entity_type_id_by_key("demo:press_machine").unwrap())
            .unwrap();
        assert_eq!(client_machine.variants.len(), 2);
        assert_eq!(
            client_machine.variants[1].placement_state,
            "demo:press[face=south,lit=off]"
        );
        assert_eq!(
            client_machine.variants[1].active[0].state,
            "demo:press[face=south,lit=on]"
        );

        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_nodelay(true).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "variant-machine".into(),
                profile: PROFILE,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap()
        else {
            panic!("expected bundle offer")
        };
        crate::client::bundle::receive(&mut peer, identity, None).unwrap();
        let (joined, _) = receive_content_manifest(&mut peer);
        assert_eq!(joined, fingerprint);
        protocol::write_client(
            &mut peer,
            &ClientMessage::ContentReady {
                fingerprint: joined,
            },
        )
        .unwrap();
        let mut player =
            InventoryProbe::new(catalog.clone(), fixture.0.join("variant-machine-probe"));
        let mut session = false;
        while !session || !player.ready(crate::world::world_to_chunk(0, 80, 2).0) {
            let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
            session |= matches!(message, ServerMessage::ActionSession { .. });
            player.accept(message);
        }
        let action_id = player.next_id();
        super::super::super::extension_lifecycle::send(
            &mut peer,
            &mut player,
            ClientMessage::Edit {
                action_id,
                x: 0,
                y: 80,
                z: 2,
                block: south,
                slot: 0,
            },
            &catalog,
        );
        super::super::super::extension_lifecycle::until(
            &mut peer,
            &mut player,
            &catalog,
            |player| {
                player.anchored([0, 80, 2]).is_some()
                    && player.block_state([0, 80, 2]) == Some(south)
                    && player.block_state([1, 80, 2]) == Some(south)
                    && player.player_count(0) == 0
            },
        );
    });
    let mut state = fixture.open().unwrap();
    assert_eq!(state.world.catalog().fingerprint(), fingerprint);
    for x in [0, 1] {
        assert_eq!(state.world.get_block(x, 80, 2).unwrap(), south);
    }
    assert!(state.inventory_store.load(PROFILE).unwrap().slots[0].is_none());
    let anchor = state
        .entities
        .anchored_at(crate::server::entities::CellCoord::new(0, 80, 2))
        .unwrap();
    let snapshot = state.entities.snapshot(anchor).unwrap();
    let payload = snapshot
        .private_payload
        .downcast_ref::<crate::server::entities::machine::MachinePayload>()
        .unwrap();
    assert_eq!(payload.variant, 1);
    std::fs::write(
        fixture.0.join("packages/demo/server/main.luau"),
        register.replace("face=south,lit=on]'", "face=south,lit=off]'"),
    )
    .unwrap();
    assert!(fixture.open().is_err());
}

#[test]
fn luau_machine_variants_reject_duplicate_and_foreign_states_before_save() {
    for invalid in [
        source().replace("face=south,lit=off]", "face=north,lit=off]"),
        source().replace("face=south,lit=on]", "bloxgloom:stone"),
        source().replacen(
            "state='demo:press[face=north,lit=off]'",
            "state='demo:press[face=north,lit=on]'",
            1,
        ),
    ] {
        let fixture = Fixture::new();
        package(&fixture, &invalid);
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/content.map").exists());
    }
}
