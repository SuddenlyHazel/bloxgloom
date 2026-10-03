//! Manual reload through the production nonblocking transport and durable actions.
use super::*;
use gameplay::{Peer, serve};
const PROFILE: u128 = 0x5c71;
use bloxgloom_host_api::actions::Request;

const REGISTER: &str = "return function(h) h.register_action('demo:probe', 1, 'Probe', 'empty', nil, 'demo:action') end";

fn action(peer: &mut Peer) -> (bool, String) {
    let request = ClientMessage::EntityInteract {
        action_id: peer.next_id(),
        target: [0; 3],
        payload: Request {
            key: "demo:probe".into(),
            version: 1,
            slot: 0,
            inventory_revision: peer.inventory.revision,
            entity: 0,
            entity_revision: 0,
            arguments: Vec::new(),
        }
        .encode()
        .unwrap(),
    };
    peer.send(&request)
}

fn reload(peer: &mut Peer) -> (bool, String) {
    protocol::write_client(&mut peer.stream, &ClientMessage::ReloadPackages).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let ServerMessage::PackageReload { reconnect, text } = peer.read(deadline)
            && text != "Validating package reload"
        {
            return (reconnect, text);
        }
    }
}

#[test]
fn package_reload_keeps_failed_revision_and_reconnects_with_inventory_and_new_behavior() {
    let fixture = Fixture::new();
    fixture.package(
        "demo",
        "requires bloxgloom:actions/v1\nmodule action action.luau",
        REGISTER,
    );
    let source = fixture.0.join("packages/demo/action.luau");
    std::fs::write(&source, "return function() error('old-revision') end").unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    state.admin_profile = Some(PROFILE);
    state.spawn_anchor = [0.5, 80.0, 0.5];
    state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
    state.world.edit(0, 80, 0, crate::world::AIR).unwrap();
    state.world.edit(0, 81, 0, crate::world::AIR).unwrap();
    state.world.edit(2, 80, 0, crate::world::AIR).unwrap();
    let catalog = state.world.catalog_arc();
    let mut inventory = Inventory::default();
    let stone = catalog.item_by_key("bloxgloom:stone").unwrap();
    assert_eq!(inventory.insert_with_catalog(stone, 128, &catalog), 0);
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    serve(state, |address| {
        let mut admin = Peer::connect(address, Arc::clone(&catalog));
        assert_eq!(admin.inventory.slots, inventory.slots);
        let mut other = Peer::connect_profile(address, Arc::clone(&catalog), 0xBAD);
        assert!(reload(&mut other).1.contains("admin profile"));
        other.stream.shutdown(Shutdown::Both).unwrap();
        assert!(!action(&mut admin).0);
        std::fs::write(&source, "invalid Luau !!").unwrap();
        let rejected = reload(&mut admin);
        assert!(
            !rejected.0 && rejected.1.contains("rejected"),
            "{rejected:?}"
        );
        assert!(!action(&mut admin).0);
        let mut late = TcpStream::connect(address).unwrap();
        late.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut late,
            &ClientMessage::Hello {
                name: "late-reload".into(),
                profile: 0x1A7E,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let (fingerprint, _) = receive_content_manifest(&mut late);
        std::fs::write(&source, "return function(c) c.set_block(2,80,0, if c.block(2,80,0).state == 'bloxgloom:air' then 'bloxgloom:glowstone' else 'bloxgloom:air') end").unwrap();
        let refreshed = reload(&mut admin);
        assert!(refreshed.0, "{refreshed:?}");
        admin.stream.shutdown(Shutdown::Both).unwrap();
        protocol::write_client(&mut late, &ClientMessage::ContentReady { fingerprint }).unwrap();
        loop {
            if let ServerMessage::PackageReload { reconnect, .. } =
                protocol::read_server_with_catalog(&mut late, &catalog).unwrap()
            {
                assert!(reconnect);
                break;
            }
        }
        late.shutdown(Shutdown::Both).unwrap();
        let mut joined = Peer::connect(address, Arc::clone(&catalog));
        assert_eq!(joined.inventory.slots, inventory.slots);
        let result = action(&mut joined);
        assert!(result.0, "{result:?}");
        // An author cannot change frozen action descriptors in a live reload.
        std::fs::write(
            fixture.0.join("packages/demo/main.luau"),
            REGISTER.replace("'Probe'", "'Changed'"),
        )
        .unwrap();
        let rejected = reload(&mut joined);
        assert!(
            !rejected.0 && rejected.1.contains("restart required"),
            "{rejected:?}"
        );
        let result = action(&mut joined);
        assert!(result.0, "{result:?}");
    });
}
