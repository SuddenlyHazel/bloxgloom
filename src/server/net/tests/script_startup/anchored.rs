//! Luau anchored behavior through the real reactor, WAL, and client ray path.
use super::*;
use crate::client::InventoryProbe;
use crate::inventory::Stack;
use crate::server::entities::{CellCoord, EntityId};
use std::time::Instant;

#[path = "anchored/failures.rs"]
mod failures;

const PROFILE: u128 = 0xAC03;
const FIRST: [i32; 3] = [0, 80, 2];
const SECOND: [i32; 3] = [2, 80, 2];

fn packages() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/anchored-counter/packages")
}

fn open(fixture: &Fixture, root: &std::path::Path) -> State {
    crate::server::server_state_with_startup(
        7,
        fixture.0.join("save"),
        2,
        ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(root)
            .unwrap(),
    )
    .unwrap()
}

fn prepare(state: &mut State) {
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -3..=4 {
        for z in -3..=4 {
            for y in 79..=82 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 79 {
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
    inventory.slots[0] = Some(Stack::new(
        state.world.catalog().item_by_key("counter:block").unwrap(),
        9,
    ));
    inventory.slots[1] = Some(Stack::new(
        state
            .world
            .catalog()
            .item_by_key("bloxgloom:stone")
            .unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
}

fn connect(
    address: std::net::SocketAddr,
    catalog: &Arc<Catalog>,
    fixture: &Fixture,
) -> (TcpStream, InventoryProbe) {
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_nodelay(true).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    protocol::write_client(
        &mut peer,
        &ClientMessage::Hello {
            name: "luau-counter".into(),
            profile: PROFILE,
            content_fingerprint: catalog.fingerprint(),
        },
    )
    .unwrap();
    let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap() else {
        panic!("expected counter bundle offer")
    };
    crate::client::bundle::receive(&mut peer, identity, None).unwrap();
    let (fingerprint, _) = receive_content_manifest(&mut peer);
    assert_eq!(fingerprint, catalog.fingerprint());
    protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint }).unwrap();
    let mut client = InventoryProbe::new(catalog.clone(), fixture.0.join("counter-probe"));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut session = false;
    while !session || !client.ready(crate::world::world_to_chunk(0, 80, 2).0) {
        assert!(Instant::now() < deadline);
        let message = protocol::read_server_with_catalog(&mut peer, catalog).unwrap();
        session |= matches!(message, ServerMessage::ActionSession { .. });
        client.accept(message);
    }
    (peer, client)
}

fn send(
    peer: &mut TcpStream,
    client: &mut InventoryProbe,
    catalog: &Catalog,
    message: ClientMessage,
) -> bool {
    let id = match &message {
        ClientMessage::Edit { action_id, .. } | ClientMessage::EntityInteract { action_id, .. } => {
            *action_id
        }
        _ => panic!("not a counter action"),
    };
    protocol::write_client_with_catalog(&mut *peer, &message, catalog).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline);
        let message = protocol::read_server_with_catalog(&mut *peer, catalog).unwrap();
        client.accept(message.clone());
        if let ServerMessage::ActionResult {
            action_id,
            accepted,
            ..
        } = message
            && action_id == id
        {
            return accepted;
        }
    }
}

fn edit(
    client: &mut InventoryProbe,
    at: [i32; 3],
    block: crate::world::BlockId,
    slot: u8,
) -> ClientMessage {
    ClientMessage::Edit {
        action_id: client.next_id(),
        x: at[0],
        y: at[1],
        z: at[2],
        block,
        slot,
    }
}

fn until(
    peer: &mut TcpStream,
    client: &mut InventoryProbe,
    catalog: &Catalog,
    ready: impl Fn(&InventoryProbe) -> bool,
) {
    super::super::extension_lifecycle::until(peer, client, catalog, ready);
}

fn counter_total(state: &mut State, at: [i32; 3]) -> u16 {
    let item = state.world.catalog().item_by_key("counter:block").unwrap();
    let held: u16 = state
        .inventory_store
        .load(PROFILE)
        .unwrap()
        .slots
        .iter()
        .flatten()
        .filter(|stack| stack.item == item)
        .map(|stack| stack.count)
        .sum();
    let dropped: u16 = crate::server::drops::nearby(&state.entities, at.map(|v| v as f32 + 0.5))
        .iter()
        .filter_map(|view| {
            crate::server::drops::stack(&state.entities, EntityId::new(view.id).unwrap())
        })
        .filter(|stack| stack.item == item)
        .map(|stack| stack.count)
        .sum();
    held + dropped
}

#[test]
fn luau_anchored_counter_cost_public_interaction_reaction_refund_and_restart_over_real_listener() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let root = packages();
    let mut ids = None;
    for restarted in [false, true] {
        let mut state = Box::new(open(&fixture, &root));
        state.spawn_anchor = [0.5, 80.0, 0.5];
        if !restarted {
            prepare(&mut state);
        }
        let catalog = state.world.catalog_arc();
        let block = catalog.state_by_key("counter:block").unwrap();
        if restarted {
            let (first_id, second_id) = ids.unwrap();
            for (at, id) in [(FIRST, first_id), (SECOND, second_id)] {
                let snapshot = state.entities.snapshot(EntityId::new(id).unwrap()).unwrap();
                assert_eq!(snapshot.anchor(), Some(CellCoord::new(at[0], at[1], at[2])));
                assert!(snapshot.next_tick.is_some());
                let descriptor = state
                    .entities
                    .types()
                    .descriptor(snapshot.entity_type)
                    .unwrap();
                let bytes = descriptor
                    .encode_payload(&snapshot.private_payload)
                    .unwrap();
                assert_eq!(bytes.len(), 9);
                assert_eq!(&bytes[5..], &80i32.to_le_bytes());
            }
        }
        gameplay::serve(state, |address| {
            let (mut peer, mut client) = connect(address, &catalog, &fixture);
            if !restarted {
                for at in [FIRST, SECOND] {
                    let place = edit(&mut client, at, block, 0);
                    assert!(send(&mut peer, &mut client, &catalog, place.clone()));
                    assert!(send(&mut peer, &mut client, &catalog, place));
                }
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(FIRST).is_some()
                        && c.anchored(SECOND).is_some()
                        && c.player_count(0) == 3
                });
                let first = client.anchored(FIRST).unwrap();
                let second = client.anchored(SECOND).unwrap();
                ids = Some((first.id, second.id));
                assert_eq!(first.payload, [0; 5]);
                let interact = client.action_on_block([0, 81, 2]);
                assert!(send(&mut peer, &mut client, &catalog, interact.clone()));
                assert!(send(&mut peer, &mut client, &catalog, interact));
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(FIRST)
                        .is_some_and(|e| e.payload == [1, 0, 0, 0, 0])
                });
                let neighbor = edit(&mut client, [1, 80, 2], crate::world::STONE, 1);
                assert!(send(&mut peer, &mut client, &catalog, neighbor));
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(FIRST)
                        .is_some_and(|e| e.payload == [1, 0, 0, 0, 1])
                });
            } else {
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(FIRST).is_some() && c.anchored(SECOND).is_some()
                });
                assert_eq!(
                    Some((
                        client.anchored(FIRST).unwrap().id,
                        client.anchored(SECOND).unwrap().id
                    )),
                    ids
                );
                assert_eq!(client.anchored(FIRST).unwrap().payload, [1, 0, 0, 0, 1]);
                let remove = edit(&mut client, [2, 81, 2], crate::world::AIR, 0);
                assert!(send(&mut peer, &mut client, &catalog, remove.clone()));
                assert!(send(&mut peer, &mut client, &catalog, remove));
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(SECOND).is_none()
                        && c.block_state([2, 80, 2]) == Some(crate::world::AIR)
                        && c.block_state([2, 81, 2]) == Some(crate::world::AIR)
                });
                let support = edit(&mut client, [0, 79, 2], crate::world::AIR, 0);
                assert!(send(&mut peer, &mut client, &catalog, support));
                until(&mut peer, &mut client, &catalog, |c| {
                    c.anchored(FIRST).is_none()
                        && c.block_state(FIRST) == Some(crate::world::AIR)
                        && c.block_state([0, 81, 2]) == Some(crate::world::AIR)
                });
            }
            let _ = peer.shutdown(Shutdown::Both);
        });
    }
    let mut recovered = open(&fixture, &root);
    for at in [FIRST, SECOND] {
        assert!(
            recovered
                .entities
                .anchored_at(CellCoord::new(at[0], at[1], at[2]))
                .is_none()
        );
        for y in [at[1], at[1] + 1] {
            assert_eq!(
                recovered.world.get_block(at[0], y, at[2]).unwrap(),
                crate::world::AIR
            );
        }
    }
    assert_eq!(
        counter_total(&mut recovered, FIRST),
        7,
        "nine items minus two costs of three plus two refunds of two"
    );
}
