//! Real fire spread -> receipted terrain -> TCP cue -> client flame geometry.
use super::*;
use crate::world::{AIR, GLOWSTONE, STONE, WOOD, world_to_chunk};
use std::collections::HashSet;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[test]
fn fire_spread_across_a_chunk_seam_reaches_the_client_renderer_after_removal() {
    thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(spread_over_tcp)
        .unwrap()
        .join()
        .unwrap();
}

fn spread_over_tcp() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let save =
        std::env::temp_dir().join(format!("bloxgloom-fire-cue-{}-{stamp}", std::process::id()));
    let mut state = Box::new(crate::server::server_state(7, save.clone()).unwrap());
    let profile = 0xF1AE;
    let position = [14.5, 95.0, 10.5];
    for x in -1..=2 {
        for y in 4..=6 {
            for z in -1..=1 {
                state
                    .world
                    .get_chunk(crate::world::ChunkKey { x, y, z })
                    .unwrap();
            }
        }
    }
    let mut edits = Vec::new();
    for x in 12..=18 {
        for z in 7..=12 {
            edits.push((x, 94, z, STONE));
            for y in 95..=98 {
                let block = if y == 95 && z == 8 && [15, 16].contains(&x) {
                    WOOD
                } else {
                    AIR
                };
                edits.push((x, y, z, block));
            }
        }
    }
    let prepared = state.world.prepare_edits(&edits).unwrap();
    state.world.apply_prepared_edits(prepared).unwrap();
    state.position_store.save(profile, position).unwrap();
    let mut inventory = crate::inventory::Inventory::default();
    inventory.slots[0] = Some(crate::inventory::Stack::new(
        crate::items::ItemId::new(GLOWSTONE.get()),
        1,
    ));
    state
        .inventory_store
        .checkpoint_snapshot(
            profile,
            &crate::server::InventoryStore::encode_snapshot(&inventory).unwrap(),
        )
        .unwrap();
    let catalog = state.world.catalog_arc();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = mpsc::sync_channel(1);
    let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stopped));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut peer = TcpStream::connect(address).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut peer,
            &ClientMessage::Hello {
                name: "fire-cue-test".into(),
                profile,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        complete_content_handshake(&mut peer);
        protocol::write_client(&mut peer, &ClientMessage::SetView { radius: 1 }).unwrap();
        let mut client = crate::client::MobileProbe::new(catalog, save.join("client-config"));
        let mut epoch = None;
        let mut owners = HashSet::from([world_to_chunk(15, 95, 8).0, world_to_chunk(16, 95, 8).0]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while epoch.is_none() || !owners.is_empty() {
            assert!(Instant::now() < deadline, "fire fixture did not stream");
            let message = protocol::read_server(&mut peer).unwrap();
            match &message {
                ServerMessage::ActionSession { epoch: value, .. } => epoch = Some(*value),
                ServerMessage::WorldSnapshotStart(start) => {
                    owners.remove(&start.chunk.key);
                }
                _ => {}
            }
            client.accept(message);
        }
        protocol::write_client(
            &mut peer,
            &ClientMessage::Edit {
                action_id: client.next_id(),
                x: 14,
                y: 95,
                z: 8,
                block: GLOWSTONE,
                slot: 0,
            },
        )
        .unwrap();
        let targets = HashSet::from([[15, 95, 8], [16, 95, 8]]);
        let mut removed = HashSet::new();
        let mut flames = HashSet::new();
        let mut rendered = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while flames != targets {
            assert!(Instant::now() < deadline, "spread produced no flame cue");
            let message = protocol::read_server(&mut peer).unwrap();
            let fire_cells = if let ServerMessage::FireBursts { cells } = &message {
                Some(cells.clone())
            } else {
                None
            };
            match &message {
                ServerMessage::WorldCommitPart(part) => {
                    for change in &part.blocks {
                        if change.block == AIR {
                            let [x, y, z] = change.local.map(i32::from);
                            removed.insert([
                                part.key.x * 16 + x,
                                part.key.y * 16 + y,
                                part.key.z * 16 + z,
                            ]);
                        }
                    }
                }
                ServerMessage::FireBursts { cells } => {
                    for cell in cells {
                        assert!(targets.contains(cell));
                        assert!(
                            removed.contains(cell),
                            "flame arrived before committed removal: {cell:?}"
                        );
                        assert!(flames.insert(*cell), "duplicate burn cue: {cell:?}");
                    }
                }
                ServerMessage::ActionResult {
                    accepted, reason, ..
                } => assert!(accepted, "ignite placement rejected: {reason}"),
                _ => {}
            }
            client.accept(message);
            if let Some(cells) = fire_cells {
                let visuals = client.fire_visuals(
                    glam::Vec3::from_array(position) + glam::Vec3::Y * 1.5,
                    glam::Vec3::NEG_Z,
                );
                for cell in cells {
                    let center = glam::Vec3::from_array(cell.map(|value| value as f32))
                        + glam::Vec3::splat(0.5);
                    let visual = *visuals
                        .iter()
                        .find(|visual| visual.center == center)
                        .expect("received fire cue must produce a visible flame");
                    assert!(visual.age < 1.0);
                    assert_eq!(visual.style, crate::render::fire::FireStyle::Flame);
                    rendered.push(visual);
                }
            }
        }
        assert_eq!(rendered.len(), 2);
        let vertices = crate::render::fire::vertices(&rendered);
        assert!(!vertices.is_empty());
        assert!(vertices.iter().all(|value| value.is_finite()));
    }));
    let _ = stop.send(());
    server.join().unwrap().unwrap();
    std::fs::remove_dir_all(save).unwrap();
    result.unwrap();
}
