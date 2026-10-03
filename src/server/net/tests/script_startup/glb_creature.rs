//! Verified packaged model, native client projection and durable controls over TCP.
use super::*;
use crate::client::{InventoryProbe, NetworkedVisualProbe};
use bloxgloom_host_api::entity::{TintMode, VisualState};

fn joined(
    probe: &mut NetworkedVisualProbe,
    kind: crate::content::EntityTypeId,
) -> protocol::PublicEntity {
    let deadline = Instant::now() + Duration::from_secs(10);
    while probe.entity(kind).is_none() {
        assert!(Instant::now() < deadline, "packaged creature join deadline");
        probe.accept_next();
    }
    probe.entity(kind).unwrap()
}

fn changed(
    probe: &mut NetworkedVisualProbe,
    kind: crate::content::EntityTypeId,
    variant: u8,
) -> VisualState {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entity = joined(probe, kind);
        if let Some(visual) = probe.model_pose(entity.id)
            && visual.variants[0] == variant
        {
            return visual;
        }
        assert!(
            Instant::now() < deadline,
            "packaged creature appearance deadline"
        );
        probe.accept_next();
    }
}

#[test]
fn packaged_glb_creature_two_clients_share_controls_and_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let packages =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/glb-creatures/packages");
    let mut saved_id = None;
    let mut fingerprint = None;
    for restarted in [false, true] {
        let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
            .with_local_packages(&packages)
            .unwrap();
        let mut state = Box::new(
            crate::server::server_state_with_startup(7, fixture.0.join("save"), 3, startup)
                .unwrap(),
        );
        state.admin_profile = Some(0xB440);
        let catalog = state.world.catalog_arc();
        let kind = catalog.entity_type_id_by_key("sprout:sproutling").unwrap();
        let definition = catalog.mobile_entity(kind).unwrap();
        assert!(definition.model.is_empty());
        assert_eq!(
            definition.authored_model.as_ref().unwrap().key,
            "sprout:model"
        );
        let asset = catalog.model_by_key("sprout:model").unwrap();
        assert_eq!(asset.model.clips.len(), 3);
        assert_eq!(asset.model.images.len(), 1);
        assert_eq!(asset.model.controls.variants.len(), 1);
        if let Some(expected) = fingerprint {
            assert_eq!(catalog.fingerprint(), expected);
        } else {
            fingerprint = Some(catalog.fingerprint());
        }
        if !restarted {
            state.spawn_anchor = [0.5, 79.0, 0.5];
            for x in -3..=3 {
                for z in -3..=3 {
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
        } else {
            let snapshot = state
                .entities
                .snapshot(crate::server::entities::EntityId::new(saved_id.unwrap()).unwrap())
                .unwrap();
            let encoded = definition
                .behavior
                .encode(&snapshot.private_payload)
                .unwrap();
            let length = u16::from_le_bytes(encoded[10..12].try_into().unwrap()) as usize;
            assert!(
                std::str::from_utf8(&encoded[12..12 + length])
                    .unwrap()
                    .ends_with(":3")
            );
            let public = definition
                .behavior
                .public(&snapshot.private_payload)
                .unwrap();
            assert_eq!(
                definition
                    .behavior
                    .visual(&public)
                    .unwrap()
                    .unwrap()
                    .variants[0],
                1
            );
        }
        gameplay::serve(state, |address| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "glb-admin".into(),
                    profile: 0xB440,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut peer).unwrap()
            else {
                panic!("expected GLB bundle")
            };
            crate::client::bundle::receive(&mut peer, identity, None).unwrap();
            let (joined_fingerprint, _) = receive_content_manifest(&mut peer);
            assert_eq!(joined_fingerprint, catalog.fingerprint());
            protocol::write_client(
                &mut peer,
                &ClientMessage::ContentReady {
                    fingerprint: joined_fingerprint,
                },
            )
            .unwrap();
            let mut admin = InventoryProbe::new(
                catalog.clone(),
                fixture.0.join(format!("admin-{restarted}")),
            );
            loop {
                let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                let ready = matches!(message, ServerMessage::ActionSession { .. });
                admin.accept(message);
                if ready {
                    break;
                }
            }
            if !restarted {
                let action_id = admin.next_id();
                protocol::write_client_with_catalog(
                    &mut peer,
                    &ClientMessage::AdminSpawnEntity {
                        action_id,
                        entity_type: kind,
                    },
                    &catalog,
                )
                .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    assert!(Instant::now() < deadline, "GLB creature spawn deadline");
                    let message = protocol::read_server_with_catalog(&mut peer, &catalog).unwrap();
                    let done = match &message {
                        ServerMessage::ActionResult {
                            action_id: id,
                            accepted,
                            reason,
                        } if *id == action_id => {
                            assert!(*accepted, "{reason}");
                            true
                        }
                        _ => false,
                    };
                    admin.accept(message);
                    if done {
                        break;
                    }
                }
            }
            let mut one = NetworkedVisualProbe::connect(
                &address.to_string(),
                0xB441,
                fixture.0.join(format!("one-{restarted}")),
            )
            .unwrap();
            let mut two = NetworkedVisualProbe::connect(
                &address.to_string(),
                0xB442,
                fixture.0.join(format!("two-{restarted}")),
            )
            .unwrap();
            for probe in [&one, &two] {
                assert_eq!(probe.catalog().fingerprint(), catalog.fingerprint());
                assert_eq!(
                    probe
                        .catalog()
                        .model_by_key("sprout:model")
                        .unwrap()
                        .fingerprint,
                    asset.fingerprint
                );
            }
            let first = joined(&mut one, kind);
            let second = joined(&mut two, kind);
            assert_eq!(first.id, second.id);
            if let Some(expected) = saved_id {
                assert_eq!(first.id, expected)
            } else {
                saved_id = Some(first.id)
            }
            if !restarted {
                assert!(one.interact(&first), "native aimed GLB creature pat failed")
            }
            let mut a = changed(&mut one, kind, 1);
            let mut b = changed(&mut two, kind, 1);
            if !restarted {
                let current = one.entity(kind).unwrap();
                assert!(one.interact(&current));
                let multiplied = changed(&mut one, kind, 0);
                let observed = changed(&mut two, kind, 0);
                assert_eq!(multiplied.tints, observed.tints);
                assert_eq!(multiplied.layers[..2], [1, 0]);
                assert_eq!(multiplied.tints[0].unwrap().mode, TintMode::Multiply);
                assert_eq!(multiplied.tints[0].unwrap().rgb, [110, 210, 140]);
                let current = one.entity(kind).unwrap();
                assert!(one.interact(&current));
                a = changed(&mut one, kind, 1);
                b = changed(&mut two, kind, 1);
            }
            assert_eq!(a.variants, b.variants);
            assert_eq!(a.layers, b.layers);
            assert_eq!(a.tints, b.tints);
            assert_eq!(a.playback, b.playback);
            assert_eq!(a.layers[..2], [0, 1]);
            let body = a.tints[0].unwrap();
            assert_eq!(body.rgb, [210, 130, 235]);
            assert_eq!(body.mode, TintMode::Replace);
            assert!(!a.playback.unwrap().looping);
            let current = one.entity(kind).unwrap();
            let schema = asset.schema();
            assert!(schema.accepts(&a));
            // The public codec contains only pose + bounded visual controls.
            assert_eq!(
                VisualState::decode(&current.payload[5..], &schema).unwrap(),
                a
            );
            assert!(current.payload.len() <= definition.max_public_bytes);
        });
    }
}
