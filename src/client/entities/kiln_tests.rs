use super::*;

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
