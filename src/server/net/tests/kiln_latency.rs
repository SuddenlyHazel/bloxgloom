//! Real-listener placement probe: distinguish durable/network latency from
//! lighting/mesh CPU work for the same scene with an idle and burning kiln.
use super::*;
use crate::client::ReplicationProbe;
use crate::inventory::{Inventory, Stack};
use crate::items::{ItemId, STICK};
use crate::world::{AIR, STONE};
use std::time::Instant;

fn observe(message: &ServerMessage, chunks: &mut ReplicationProbe) {
    chunks.accept(message.clone());
}

fn action(
    peer: &mut TcpStream,
    chunks: &mut ReplicationProbe,
    message: ClientMessage,
    id: u128,
) -> Duration {
    let start = Instant::now();
    protocol::write_client(&mut *peer, &message).unwrap();
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "action acknowledgement stalled"
        );
        let message = protocol::read_server(&mut *peer).unwrap();
        observe(&message, chunks);
        if let ServerMessage::ActionResult {
            action_id,
            accepted,
            reason,
        } = message
            && action_id == id
        {
            assert!(accepted, "placement probe action failed: {reason}");
            return start.elapsed();
        }
    }
}

#[test]
fn running_kiln_keeps_nearby_and_cross_chunk_placements_live() {
    placement_probe(false, false);
}

#[test]
fn running_hopper_feeds_kiln_while_player_moves_and_places_over_real_tcp() {
    placement_probe(true, false);
}

#[test]
fn chest_collects_hopper_output_while_moving_and_building_over_real_tcp() {
    placement_probe(true, true);
}

fn placement_probe(with_hopper: bool, with_chest: bool) {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save = std::env::temp_dir().join(format!(
        "bloxgloom-kiln-placement-{}-{suffix}",
        std::process::id()
    ));
    let mut state = Box::new(crate::server::server_state(7, save.clone()).unwrap());
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -3..=3 {
        for z in -1..=4 {
            for y in 79..=82 {
                state
                    .world
                    .edit(
                        x,
                        y,
                        z,
                        if y == 79 && !(with_chest && x == 0 && z == 1) {
                            STONE
                        } else {
                            AIR
                        },
                    )
                    .unwrap();
            }
        }
    }
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(crate::content::KILN_ITEM, 1));
    inventory.slots[1] = Some(Stack::new(STICK, 1));
    inventory.slots[2] = Some(Stack::new(ItemId(crate::world::GRAVEL.0), 128));
    inventory.slots[3] = Some(Stack::new(ItemId(STONE.0), 128));
    inventory.slots[4] = Some(Stack::new(crate::content::HOPPER_ITEM, 2));
    inventory.slots[5] = Some(Stack::new(crate::content::CHEST_ITEM, 1));
    state.inventory_store.save(0xFACE, &inventory).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (stop_tx, stop_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_nodelay(true).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "kiln-latency".into(),
                profile: 0xFACE,
                content_fingerprint: crate::content::catalog().fingerprint(),
            },
        )
        .unwrap();
        complete_content_handshake(&mut peer);
        let mut chunks = ReplicationProbe::new();
        let near = crate::world::world_to_chunk(1, 80, 3).0;
        let far = crate::world::world_to_chunk(-1, 80, 3).0;
        let mut epoch = None;
        let deadline = Instant::now() + Duration::from_secs(10);
        while epoch.is_none() || !chunks.contains_key(&near) || !chunks.contains_key(&far) {
            assert!(Instant::now() < deadline, "initial streaming stalled");
            let message = protocol::read_server(&mut peer).unwrap();
            if let ServerMessage::ActionSession { epoch: value, .. } = message {
                epoch = Some(value);
            }
            observe(&message, &mut chunks);
        }
        let mut sequence = 0u64;
        let mut next_id = || {
            sequence += 1;
            (u128::from(epoch.unwrap()) << 64) | u128::from(sequence)
        };
        let id = next_id();
        action(
            &mut peer,
            &mut chunks,
            ClientMessage::Edit {
                action_id: id,
                x: 0,
                y: 80,
                z: 1,
                block: crate::content::KILN_DEFAULT_STATE,
                slot: 0,
            },
            id,
        );
        for burning in [false, true] {
            if burning {
                if with_chest {
                    for (y, block, slot) in [
                        (78, crate::content::CHEST_STATE, 5),
                        (79, crate::content::HOPPER_STATE, 4),
                    ] {
                        let id = next_id();
                        action(
                            &mut peer,
                            &mut chunks,
                            ClientMessage::Edit {
                                action_id: id,
                                x: 0,
                                y,
                                z: 1,
                                block,
                                slot,
                            },
                            id,
                        );
                    }
                }
                if with_hopper {
                    let id = next_id();
                    action(
                        &mut peer,
                        &mut chunks,
                        ClientMessage::Edit {
                            action_id: id,
                            x: 0,
                            y: 82,
                            z: 1,
                            block: crate::content::HOPPER_STATE,
                            slot: 4,
                        },
                        id,
                    );
                }
                for mut payload in [vec![1, 0, 0, 1, 1, 0], vec![1, 0, 1, 2, 128, 0]] {
                    let id = next_id();
                    let target = if with_hopper { [0, 82, 1] } else { [0, 80, 1] };
                    if with_hopper {
                        let entity = chunks.workstation(target);
                        payload[0] = 2;
                        payload.extend(entity.id.to_le_bytes());
                        payload.extend(entity.revision.to_le_bytes());
                    }
                    action(
                        &mut peer,
                        &mut chunks,
                        ClientMessage::EntityInteract {
                            action_id: id,
                            target,
                            payload,
                        },
                        id,
                    );
                }
                let deadline = Instant::now() + Duration::from_secs(10);
                while chunks[&near].block([0, 0, 1])
                    != Some(crate::content::BlockStateId(
                        crate::content::KILN_DEFAULT_STATE.0 + 1,
                    ))
                {
                    assert!(Instant::now() < deadline, "kiln did not start burning");
                    observe(&protocol::read_server(&mut peer).unwrap(), &mut chunks);
                }
            }
            for (name, x, key) in [("same-chunk", 1, near), ("adjacent-chunk", -1, far)] {
                for place in [true, false, true, false] {
                    let id = next_id();
                    protocol::write_client(
                        &mut peer,
                        &ClientMessage::Move {
                            seq: id as u64,
                            dx: if place { 0.1 } else { -0.1 },
                            dy: 0.0,
                            dz: 0.0,
                        },
                    )
                    .unwrap();
                    let ack = action(
                        &mut peer,
                        &mut chunks,
                        ClientMessage::Edit {
                            action_id: id,
                            x,
                            y: 80,
                            z: 3,
                            block: if place { STONE } else { AIR },
                            slot: 3,
                        },
                        id,
                    );
                    let light_start = Instant::now();
                    let light = crate::lighting::LightField::build_with_bounce_and_catalog(
                        key,
                        &chunks,
                        7,
                        true,
                        crate::content::catalog(),
                    );
                    let lighting = light_start.elapsed();
                    let mesh_start = Instant::now();
                    let _mesh = crate::render::mesh_chunk_lit_with_catalog(
                        &chunks[&key],
                        &light,
                        1,
                        crate::content::catalog(),
                    );
                    eprintln!(
                        "kiln burning={burning} {name} place={place}: ack={ack:?}, lighting={lighting:?}, mesh={:?}",
                        mesh_start.elapsed()
                    );
                    assert_eq!(
                        chunks[&key].block(crate::world::world_to_chunk(x, 80, 3).1),
                        Some(if place { STONE } else { AIR })
                    );
                }
            }
        }
        if with_chest {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let chest = chunks.workstation([0, 78, 1]);
                let view =
                    crate::protocol::workstation::WorkstationView::decode(&chest.payload).unwrap();
                assert_eq!(
                    view.kind,
                    crate::protocol::workstation::WorkstationKind::Chest
                );
                if view
                    .slots
                    .iter()
                    .flatten()
                    .any(|s| s.item == ItemId(STONE.0))
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "finished stone did not reach chest"
                );
                observe(&protocol::read_server(&mut peer).unwrap(), &mut chunks);
            }
        }
        let _ = peer.shutdown(Shutdown::Both);
    }));
    let _ = stop_tx.send(());
    server.join().unwrap().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
