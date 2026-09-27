use super::*;

#[test]
fn production_rf_shortcuts_emit_registered_identity_and_inventory_fences() {
    let anchor = [0, 79, 0];
    let upper = crate::world::world_to_chunk(0, 80, 0).0;
    let mut app = crate::client::ClientApp::new(
        crate::client::Network::disconnected_for_test(),
        crate::config::Config::default(),
        std::env::temp_dir().join("unused-registered-hotkey-config"),
    );
    app.accept(crate::protocol::ServerMessage::ActionSession {
        epoch: 7,
        next_seq: 1,
        acked_seq: 0,
    });
    app.position = glam::Vec3::new(0.5, 79.0, 2.5);
    app.yaw = -std::f32::consts::FRAC_PI_2;
    app.pitch = 0.0;
    app.grabbed = true;
    app.inventory.revision = 13;
    app.config.selected_slot = 4;
    let entity = PublicEntity {
        id: 91,
        entity_type: crate::content::KILN_ENTITY_TYPE,
        revision: 9,
        motion_revision: 0,
        location: PublicEntityLocation::Anchored {
            anchor,
            anchor_state: crate::content::KILN_DEFAULT_STATE,
        },
        payload: vec![],
    };
    app.replicas
        .entities
        .insert(upper, BTreeMap::from([(91, entity)]));
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    blocks[Chunk::index([0, 0, 0]).unwrap()] = crate::content::KILN_DEFAULT_STATE;
    app.chunks
        .insert(upper, Arc::new(Chunk::from_blocks(upper, 1, blocks)));
    for (verb, direction, slot) in [
        (kiln::INSERT_INPUT, 0, 1),
        (kiln::TAKE_OUTPUT, 1, 2),
        (kiln::INSERT_FUEL, 0, 0),
        (kiln::TAKE_FUEL, 1, 0),
    ] {
        app.interact_aimed_entity(verb);
        let ClientMessage::EntityInteract {
            payload, target, ..
        } = app.pending_commands.pop_front().unwrap()
        else {
            panic!()
        };
        let request = bloxgloom_host_api::actions::Request::decode(&payload).unwrap();
        assert_eq!(target, [0, 80, 0]);
        assert_eq!(request.key, "bloxgloom:kiln/inventory");
        assert_eq!(
            (
                request.entity,
                request.entity_revision,
                request.inventory_revision,
                request.slot
            ),
            (91, 9, 13, 4)
        );
        assert_eq!(request.arguments, vec![direction, slot, 1, 0]);
    }
    app.config_writer.finish();
}

#[test]
fn workstation_resolves_both_halves_and_closes_on_replacement() {
    let anchor = [0, 79, 0];
    let lower = crate::world::world_to_chunk(0, 79, 0).0;
    let upper = crate::world::world_to_chunk(0, 80, 0).0;
    let kiln = PublicEntity {
        id: 91,
        entity_type: crate::content::KILN_ENTITY_TYPE,
        revision: 1,
        motion_revision: 1,
        location: PublicEntityLocation::Anchored {
            anchor,
            anchor_state: crate::content::KILN_DEFAULT_STATE,
        },
        payload: crate::protocol::workstation::WorkstationView {
            status: vec![0, 0],
            ..Default::default()
        }
        .encode(),
    };
    let mut app = crate::client::ClientApp::new(
        crate::client::Network::disconnected_for_test(),
        crate::config::Config::default(),
        std::env::temp_dir().join("unused-kiln-ui-test"),
    );
    app.replicas
        .entities
        .insert(lower, BTreeMap::from([(91, kiln.clone())]));
    app.replicas
        .entities
        .insert(upper, BTreeMap::from([(91, kiln.clone())]));
    assert_eq!(app.replicas.kiln_at(anchor, &app.catalog).unwrap().id, 91);
    assert_eq!(
        app.replicas.kiln_at([0, 80, 0], &app.catalog).unwrap().id,
        91
    );
    assert!(app.replicas.kiln_at([0, 81, 0], &app.catalog).is_none());
    app.position = glam::Vec3::new(0.5, 79.0, 2.5);
    app.set_screen(crate::ui::UiScreen::Container);
    app.kiln_target = Some(([0, 80, 0], 91));
    app.validate_kiln_screen();
    assert_eq!(app.screen, crate::ui::UiScreen::Container);
    app.replicas.entities.insert(
        upper,
        BTreeMap::from([(92, PublicEntity { id: 92, ..kiln })]),
    );
    app.validate_kiln_screen();
    assert_eq!(app.screen, crate::ui::UiScreen::Playing);
    app.config_writer.finish();
}
