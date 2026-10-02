//! A delivered package combines weather planning, material metadata and public audio state.
use super::*;
use crate::protocol::{PublicEntity, PublicEntityChange};
fn next_collector(peer: &mut Peer, predicate: impl Fn(&PublicEntity) -> bool) -> PublicEntity {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let ServerMessage::WorldCommitPart(part) = peer.read(deadline) {
            for change in part.entities {
                if let PublicEntityChange::Upsert(entity) = change
                    && peer.catalog.entity_type(entity.entity_type).unwrap().key
                        == "rain:collector_state"
                    && predicate(&entity)
                {
                    return entity;
                }
            }
        }
    }
}
#[test]
fn rain_collector_collects_under_server_weather_and_pauses_when_cleared() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/rain-collector/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&root)
        .unwrap();
    let mut state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    );
    state.spawn_anchor = [0.5, 80., 0.5];
    state.admin_profile = Some(PROFILE);
    for x in 0..=2 {
        for y in 79..=144 {
            state
                .world
                .edit(x, y, 0, if y == 79 { crate::world::STONE } else { AIR })
                .unwrap();
        }
    }
    state.world.edit(2, 82, 0, crate::world::STONE).unwrap();
    let change = state.weather.prepare(4, 0).unwrap();
    state.weather.apply(change).unwrap();
    let catalog = state.world.catalog_arc();
    let collector = catalog.state_by_key("rain:collector").unwrap();
    let acoustic = catalog.block_acoustics(collector).unwrap();
    assert_eq!(
        acoustic.surface,
        bloxgloom_host_api::content::RainSurface::Metal
    );
    assert!(acoustic.impact.is_some());
    let inert = state
        .client_bundle
        .as_ref()
        .unwrap()
        .session_catalog()
        .unwrap();
    let resolved = crate::content::ContentManifest::from_catalog(&catalog)
        .resolve_catalog(&inert)
        .unwrap();
    assert_eq!(resolved.fingerprint(), catalog.fingerprint());
    assert_eq!(resolved.block_acoustics(collector), Some(acoustic));
    let prepared =
        crate::client::startup::prepare(Arc::clone(state.client_bundle.as_ref().unwrap())).unwrap();
    let mut visual = crate::client::presentation::VisualSession::with_parameters(
        Arc::clone(prepared.replica.as_ref().unwrap()),
        Default::default(),
    )
    .unwrap();
    let mut inventory = Inventory::default();
    inventory.slots[0] = Some(Stack::new(
        catalog.item_by_key("rain:collector").unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inventory).unwrap();
    serve(state, |address| {
        let negotiated =
            crate::client::connect_catalog_probe(&address.to_string(), PROFILE + 1).unwrap();
        assert_eq!(negotiated.fingerprint(), catalog.fingerprint());
        let mut peer = Peer::connect(address, catalog.clone());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::WorldSnapshotStart(start) = peer.read(deadline)
                && start.chunk.key == (crate::world::ChunkKey { x: 0, y: 5, z: 0 })
            {
                break;
            }
        }
        let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
        peer.sequence += 1;
        assert!(
            peer.send(&ClientMessage::Edit {
                action_id,
                x: 2,
                y: 80,
                z: 0,
                block: collector,
                slot: 0
            })
            .0
        );
        let sheltered = next_collector(&mut peer, |e| e.payload == [0, 0]);
        assert_eq!(sheltered.payload, [0, 0], "covered collector filled");
        let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
        peer.sequence += 1;
        assert!(
            peer.send(&ClientMessage::Edit {
                action_id,
                x: 2,
                y: 82,
                z: 0,
                block: AIR,
                slot: 0
            })
            .0
        );
        let running = next_collector(&mut peer, |e| e.payload[0] > 0 && e.payload[1] == 1);
        let position = match running.location {
            crate::protocol::PublicEntityLocation::Mobile { position } => position,
            _ => unreachable!(),
        };
        visual.entities(
            vec![crate::client::presentation::EntityView {
                id: running.id,
                key: "rain:collector_state".into(),
                position,
                revision: running.revision,
                motion_revision: running.motion_revision,
                public: running.payload.clone(),
            }],
            1,
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            visual.poll();
            let sounds = visual.take_sounds();
            if !sounds.is_empty() {
                assert!(
                    matches!(&sounds[0].kind,bloxgloom_host_api::sound::Kind::Play{clip,entity:Some(id),looping:true,..} if clip=="rain:motor" && *id==running.id)
                );
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
        peer.sequence += 1;
        assert!(
            peer.send(&ClientMessage::EntityInteract {
                action_id,
                target: [0, 0, 0],
                payload: Request {
                    key: crate::gameplay::admin::WEATHER.into(),
                    version: 1,
                    slot: 0,
                    inventory_revision: peer.inventory.revision,
                    entity: 0,
                    entity_revision: 0,
                    arguments: vec![0, 0, 0, 0, 0]
                }
                .encode()
                .unwrap()
            })
            .0
        );
        let paused = next_collector(&mut peer, |e| e.id == running.id && e.payload[1] == 0);
        assert_eq!(
            paused.payload[0], running.payload[0],
            "clear weather added water"
        );
        visual.entities(
            vec![crate::client::presentation::EntityView {
                id: paused.id,
                key: "rain:collector_state".into(),
                position,
                revision: paused.revision,
                motion_revision: paused.motion_revision,
                public: paused.payload,
            }],
            1,
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            visual.poll();
            let sounds = visual.take_sounds();
            if !sounds.is_empty() {
                assert!(matches!(
                    sounds[0].kind,
                    bloxgloom_host_api::sound::Kind::Stop
                ));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    });
}
