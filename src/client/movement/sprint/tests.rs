use super::*;

const OWN: u64 = (1 << 63) | 1;

fn app() -> (ClientApp, std::sync::mpsc::Receiver<ClientMessage>) {
    let (network, outgoing) = Network::capture_outgoing_for_test();
    let path = std::env::temp_dir().join(format!(
        "sprint-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let mut app = ClientApp::new(network, Config::default(), path);
    app.screen = UiScreen::Playing;
    app.grabbed = true;
    app.flight.flying = false;
    app.owned_entity_id = Some(OWN);
    (app, outgoing)
}

fn double_tap(app: &mut ClientApp, now: Instant) {
    app.forward_input(true, false, now);
    app.forward_input(false, false, now + Duration::from_millis(50));
    app.forward_input(true, false, now + Duration::from_millis(100));
}

#[test]
fn sprint_double_tap_requires_a_release_and_server_approval_then_stops_on_release() {
    let (mut app, outgoing) = app();
    let now = Instant::now();
    app.forward_input(true, false, now);
    app.forward_input(true, true, now + Duration::from_millis(50));
    app.forward_input(true, false, now + Duration::from_millis(80));
    assert!(
        outgoing.try_recv().is_err(),
        "holding/repeat is not a second tap"
    );
    app.forward_input(false, false, now + Duration::from_millis(100));
    app.forward_input(true, false, now + Duration::from_millis(301));
    assert!(outgoing.try_recv().is_err(), "late tap only walks");
    app.forward_input(false, false, now + Duration::from_millis(330));
    app.forward_input(true, false, now + Duration::from_millis(400));
    assert_eq!(
        outgoing.try_recv().unwrap(),
        ClientMessage::SetSprinting { sprinting: true }
    );
    assert!(!app.sprinting());
    app.accept(ServerMessage::PlayerSprint {
        entity_id: OWN,
        sprinting: true,
    });
    assert!(app.sprinting());
    app.move_player(0.01);
    let ClientMessage::Move { dx, dy, dz, .. } = outgoing.try_recv().unwrap() else {
        panic!("move expected")
    };
    assert!((Vec3::new(dx, dy, dz).length() - 0.12).abs() < 0.0001);
    app.forward_input(false, false, now + Duration::from_millis(500));
    assert!(
        !app.sprinting(),
        "release immediately restores walk prediction"
    );
    assert_eq!(
        outgoing.try_recv().unwrap(),
        ClientMessage::SetSprinting { sprinting: false }
    );
}

#[test]
fn sprint_quick_restart_survives_older_release_acknowledgement() {
    let (mut app, outgoing) = app();
    let now = Instant::now();
    double_tap(&mut app, now);
    outgoing.try_recv().unwrap();
    app.accept_sprint(OWN, true);
    app.forward_input(false, false, now + Duration::from_millis(150));
    outgoing.try_recv().unwrap();
    app.forward_input(true, false, now + Duration::from_millis(200));
    outgoing.try_recv().unwrap();
    app.accept_sprint(OWN, false);
    assert!(
        app.sprint.requested,
        "old release must not discard new intent"
    );
    app.accept_sprint(OWN, true);
    assert!(app.sprinting());
    app.retire_session();
    assert!(!app.sprint.requested && app.player_sprints.is_empty() && !app.keys.forward);
}

#[test]
fn sprint_cancels_for_crouch_backward_focus_menus_flight_and_denial() {
    for stop in 0..6 {
        let (mut app, outgoing) = app();
        double_tap(&mut app, Instant::now());
        outgoing.try_recv().unwrap();
        app.accept_sprint(OWN, true);
        match stop {
            0 => app.request_crouch(true),
            1 => app.backward_input(true),
            2 => app.set_grab(false),
            3 => app.set_screen(UiScreen::Admin),
            4 => {
                app.admin_enabled = true;
                app.toggle_flying();
            }
            _ => app.accept_sprint(OWN, false),
        }
        assert!(!app.sprinting(), "stop {stop}");
        assert!(app.sprint.last_press.is_none());
        if stop != 5 {
            assert_eq!(
                outgoing.try_recv().unwrap(),
                ClientMessage::SetSprinting { sprinting: false }
            );
        }
    }
    let (mut app, outgoing) = app();
    app.flight.flying = true;
    double_tap(&mut app, Instant::now());
    assert!(
        outgoing.try_recv().is_err(),
        "sprinting requires walking mode"
    );
}

pub(crate) fn exercise_player_sprint(address: &str, path: PathBuf) {
    let profile = 0x5c72;
    let network = Network::connect(address, 1, profile).unwrap();
    let mut app = ClientApp::new(
        network,
        Config {
            profile,
            ..Default::default()
        },
        path,
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut inventory = false;
    while app.actions.epoch == 0 || !inventory || app.owned_entity_id.is_none() {
        inventory |= consume(&mut app, deadline);
    }
    let entity = app.owned_entity_id.unwrap();
    app.admin_enabled = true;
    app.screen = UiScreen::Admin;
    app.toggle_flying();
    while app.flight.pending.is_some() {
        consume(&mut app, deadline);
    }
    assert!(!app.flight.flying);
    app.screen = UiScreen::Playing;
    app.grabbed = true;
    app.yaw = 0.0;
    assert_eq!(app.position, Vec3::new(0.5, 85.0, 0.5));
    app.forward_input(true, false, Instant::now());
    app.move_player(0.01);
    while !app.unacked.is_empty() {
        consume(&mut app, deadline);
    }
    assert!((app.position.x - 0.58).abs() < 0.001);
    let now = Instant::now();
    app.forward_input(false, false, now);
    app.cancel_sprint();
    double_tap(&mut app, now);
    while !app.sprinting() {
        consume(&mut app, deadline);
    }
    app.move_player(0.01);
    while !app.unacked.is_empty() {
        consume(&mut app, deadline);
    }
    assert!(
        (app.position.x - 0.70).abs() < 0.001,
        "native sprint predicts and reconciles at 12 blocks/s"
    );
    assert_eq!(app.position.y, 85.0);

    // A newly connected native peer receives the active sprint snapshot.
    let observer = Network::connect(address, 1, 0x5c73).unwrap();
    expect_sprint(&observer, entity, true, deadline);
    app.forward_input(false, false, now + Duration::from_millis(50));
    while app.player_sprints.contains_key(&entity) {
        consume(&mut app, deadline);
    }
    expect_sprint(&observer, entity, false, deadline);
    app.forward_input(true, false, now + Duration::from_millis(800));
    app.move_player(0.01);
    while !app.unacked.is_empty() {
        consume(&mut app, deadline);
    }
    assert!((app.position.x - 0.78).abs() < 0.001);

    app.forward_input(false, false, now + Duration::from_millis(850));
    app.forward_input(true, false, now + Duration::from_millis(900));
    while !app.sprinting() {
        consume(&mut app, deadline);
    }
    expect_sprint(&observer, entity, true, deadline);
    app.request_crouch(true);
    while app.player_sprints.contains_key(&entity) || !app.crouching() {
        consume(&mut app, deadline);
    }
    expect_sprint(&observer, entity, false, deadline);
    assert!(!app.sprinting());
    app.request_crouch(false);
    while app.crouching() {
        consume(&mut app, deadline);
    }
    app.forward_input(false, false, now + Duration::from_millis(950));
    double_tap(&mut app, now + Duration::from_millis(1000));
    while !app.sprinting() {
        consume(&mut app, deadline);
    }
    expect_sprint(&observer, entity, true, deadline);
    app.retire_session();
    expect_sprint(&observer, entity, false, deadline);
}

fn consume(app: &mut ClientApp, deadline: Instant) -> bool {
    match app
        .network
        .incoming
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("sprint client deadline")
    {
        Incoming::Message(message) => {
            let inventory = matches!(*message, ServerMessage::Inventory { .. });
            app.accept(*message);
            assert!(!app.disconnected, "{:?}", app.failure);
            inventory
        }
        Incoming::Closed(reason) => panic!("{reason}"),
    }
}

fn expect_sprint(network: &Network, entity: u64, expected: bool, deadline: Instant) {
    loop {
        match network
            .incoming
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("sprint observer deadline")
        {
            Incoming::Message(message) => {
                if matches!(*message, ServerMessage::PlayerSprint { entity_id, sprinting } if entity_id == entity && sprinting == expected)
                {
                    return;
                }
            }
            Incoming::Closed(reason) => panic!("{reason}"),
        }
    }
}
