use super::*;
use crate::world::{AIR, CHUNK_SIZE, CHUNK_VOLUME, STONE};

fn app() -> ClientApp {
    ClientApp::new(
        Network::disconnected_for_test(),
        Config::default(),
        std::env::temp_dir().join(format!("typed-observations-{}", std::process::id())),
    )
}

#[test]
fn inventory_zero_is_known_and_stale_or_duplicate_updates_do_not_replace_it() {
    let mut app = app();
    assert!(app.observations.inventory.is_none());
    app.accept(ServerMessage::Inventory {
        revision: 0,
        slots: std::array::from_fn(|_| None),
    });
    let initial = app.observations.inventory.as_ref().unwrap();
    assert_eq!(initial.revision, 0);
    assert_eq!(initial.slots.len(), 36);
    assert!(initial.slots.iter().all(|slot| slot.stack.is_none()));
    app.accept(ServerMessage::Inventory {
        revision: 4,
        slots: std::array::from_fn(|_| None),
    });
    let accepted = Arc::clone(&app.observations);
    app.accept(ServerMessage::Inventory {
        revision: 3,
        slots: std::array::from_fn(|_| None),
    });
    assert!(Arc::ptr_eq(&accepted, &app.observations));
    app.accept(ServerMessage::Inventory {
        revision: 4,
        slots: std::array::from_fn(|_| None),
    });
    assert!(Arc::ptr_eq(&accepted, &app.observations));
}

#[test]
fn only_contiguous_installed_deltas_publish_authoritative_cells() {
    let mut app = app();
    let key = ChunkKey { x: -1, y: 5, z: 0 };
    app.accept(ServerMessage::Chunk(Chunk::from_blocks(
        key,
        5,
        vec![AIR; CHUNK_VOLUME],
    )));
    assert!(app.observations.blocks.is_empty());
    let delta = |version| ServerMessage::Delta {
        key,
        version,
        x: 1,
        y: 2,
        z: 3,
        block: STONE,
    };
    app.accept(delta(7));
    assert!(app.observations.blocks.is_empty());
    assert_eq!(app.chunks[&key].version, 5);
    app.accept(delta(6));
    assert_eq!(app.observations.blocks.len(), 1);
    let view = &app.observations.blocks[0];
    assert_eq!(
        view.position,
        [1 - CHUNK_SIZE as i32, 5 * CHUNK_SIZE as i32 + 2, 3]
    );
    assert_eq!(view.state, "bloxgloom:stone");
    assert_eq!(view.version, 6);
    let accepted = Arc::clone(&app.observations);
    app.accept(delta(6));
    app.accept(delta(9));
    assert!(Arc::ptr_eq(&accepted, &app.observations));
    assert_eq!(app.chunks[&key].version, 6);
    app.observe_installed_blocks(
        &[],
        vec![(
            ChunkKey {
                x: 100,
                y: 100,
                z: 100,
            },
            [0; 3],
        )],
        false,
        None,
    );
    assert_eq!(app.observations.blocks.len(), 1);
}

#[test]
fn recent_cells_are_bounded_refreshed_by_snapshots_and_removed_on_eviction() {
    let mut app = app();
    let key = ChunkKey { x: 0, y: 5, z: 0 };
    app.accept(ServerMessage::Chunk(Chunk::from_blocks(
        key,
        1,
        vec![AIR; CHUNK_VOLUME],
    )));
    let cells = (0..68)
        .map(|index| {
            (
                key,
                [(index % CHUNK_SIZE) as u8, (index / CHUNK_SIZE) as u8, 0],
            )
        })
        .collect();
    app.observe_installed_blocks(&[key], cells, false, None);
    assert_eq!(app.observations.blocks.len(), 64);
    assert!(app.observations.blocks_truncated);
    assert_eq!(app.observations.blocks[0].position[0], 4);
    app.accept(ServerMessage::Chunk(Chunk::from_blocks(
        key,
        2,
        vec![STONE; CHUNK_VOLUME],
    )));
    assert!(
        app.observations
            .blocks
            .iter()
            .all(|view| view.version == 2 && view.state == "bloxgloom:stone")
    );
    app.chunks.remove(&key);
    app.prune_observed_blocks();
    assert!(app.observations.blocks.is_empty());
}

fn pending(app: &mut ClientApp, key: Option<&str>) -> u128 {
    let id = app.actions.allocate().unwrap();
    if let Some(key) = key {
        let payload = bloxgloom_host_api::actions::Request {
            key: key.into(),
            version: 1,
            slot: 0,
            inventory_revision: 0,
            entity: 0,
            entity_revision: 0,
            arguments: vec![],
        }
        .encode()
        .unwrap();
        app.pending_actions.insert(
            id,
            ClientMessage::EntityInteract {
                action_id: id,
                target: [0; 3],
                payload,
            },
        );
    }
    id
}

#[test]
fn receipts_retain_package_ownership_deduplicate_and_retire_with_the_session() {
    let mut app = app();
    app.actions.install_fresh_session(11, 1, 0).unwrap();
    for index in 0..20 {
        let id = pending(
            &mut app,
            Some(if index % 2 == 0 {
                "demo:use"
            } else {
                "other:use"
            }),
        );
        app.accept(ServerMessage::ActionResult {
            action_id: id,
            accepted: true,
            reason: String::new(),
        });
        let accepted = Arc::clone(&app.observations);
        app.accept(ServerMessage::ActionResult {
            action_id: id,
            accepted: true,
            reason: String::new(),
        });
        assert!(Arc::ptr_eq(&accepted, &app.observations));
    }
    assert_eq!(app.observations.actions.len(), 16);
    let own = app.observations.for_owner("demo");
    assert_eq!(own.actions.len(), 8);
    assert!(
        own.actions
            .iter()
            .all(|receipt| receipt.key.as_deref() == Some("demo:use"))
    );
    let native = pending(&mut app, None);
    app.accept(ServerMessage::ActionResult {
        action_id: native,
        accepted: false,
        reason: "denied".into(),
    });
    assert!(app.observations.actions.last().unwrap().key.is_none());
    assert!(
        app.observations
            .for_owner("demo")
            .actions
            .iter()
            .all(|receipt| receipt.id != native)
    );
    app.retire_session();
    assert!(app.observations.inventory.is_none());
    assert!(app.observations.world.is_none());
    assert!(app.observations.blocks.is_empty());
    assert!(app.observations.actions.is_empty());
}

#[test]
fn ui_less_callbacks_receive_latest_world_snapshot_as_readonly_data() {
    let mut app = app();
    let script = Arc::new(presentation::Script {
        module: "demo@1:visuals".into(),
        source: r#"return function(e)
            assert(e.replica.inventory and #e.replica.inventory.slots==36)
            if e.event=='replica:inventory' then assert(e.replica.world==nil)
            else
                assert(e.event=='replica:world' and e.replica.world.elapsed_ms==300000)
                assert(e.replica.world.cycle_ms==1200000)
                assert(not pcall(function() e.replica.world.elapsed_ms=1 end))
                assert(not pcall(function() e.replica.inventory.slots[1].slot=2 end))
            end
            return {}
        end"#
            .into(),
    });
    let mut visual = presentation::VisualSession::new(script).unwrap();
    visual.enable_observation_events();
    app.visual_session = Some(visual);
    app.accept(ServerMessage::Inventory {
        revision: 0,
        slots: std::array::from_fn(|_| None),
    });
    app.accept(ServerMessage::WorldTime { elapsed_ms: 300000 });
    let visual = app.visual_session.as_mut().unwrap();
    visual.wait_for_test().unwrap();
    visual.poll();
    visual.wait_for_test().unwrap();
}

#[test]
fn terrain_action_receipts_preserve_the_registered_package_key() {
    let mut app = app();
    app.actions.install_fresh_session(12, 1, 0).unwrap();
    let id = pending(&mut app, Some("demo:terrain_use"));
    let ClientMessage::EntityInteract { payload, .. } = app.pending_actions.get_mut(&id).unwrap()
    else {
        unreachable!()
    };
    let request = bloxgloom_host_api::actions::Request::decode(payload).unwrap();
    *payload = bloxgloom_host_api::actions::TerrainRequest {
        version: 9,
        request,
    }
    .encode()
    .unwrap();
    app.accept(ServerMessage::ActionResult {
        action_id: id,
        accepted: true,
        reason: String::new(),
    });
    let own = app.observations.for_owner("demo");
    assert_eq!(own.actions.len(), 1);
    assert_eq!(own.actions[0].key.as_deref(), Some("demo:terrain_use"));
    assert!(app.observations.for_owner("other").actions.is_empty());
}
