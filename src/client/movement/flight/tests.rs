use super::*;

fn app() -> (ClientApp, std::sync::mpsc::Receiver<ClientMessage>) {
    let (network, outgoing) = Network::capture_outgoing_for_test();
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-flight-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    (ClientApp::new(network, Config::default(), path), outgoing)
}

#[test]
fn flying_toggle_waits_for_authority_and_repeated_clicks_do_not_queue() {
    let (mut app, outgoing) = app();
    app.admin_enabled = true;
    app.screen = UiScreen::Admin;
    app.activate_control(None, UiControl::AdminFlying);
    assert_eq!(
        outgoing.try_recv().unwrap(),
        ClientMessage::SetFlying { flying: false }
    );
    assert!(
        app.flight.flying,
        "an unacknowledged toggle must not switch prediction"
    );
    assert_eq!(app.flight.pending, Some(false));
    app.activate_control(None, UiControl::AdminFlying);
    assert!(outgoing.try_recv().is_err());
    app.unacked.push_back((1, Vec3::new(0.1, 0.2, 0.0)));
    app.accept(ServerMessage::FlyingMode { flying: false });
    assert!(!app.flight.flying);
    assert_eq!(app.flight.pending, None);
    assert_eq!(app.unacked[0].1, Vec3::new(0.1, 0.0, 0.0));
    app.activate_control(None, UiControl::AdminFlying);
    assert_eq!(
        outgoing.try_recv().unwrap(),
        ClientMessage::SetFlying { flying: true }
    );
    app.accept(ServerMessage::FlyingMode { flying: true });
    assert!(app.flight.flying);
    app.retire_session();
    assert!(app.flight.flying && app.flight.pending.is_none());
}

#[test]
fn walking_jump_and_non_admin_toggle_respect_session_input() {
    let (mut app, outgoing) = app();
    app.toggle_flying();
    assert!(outgoing.try_recv().is_err());
    app.flight.flying = false;
    app.grabbed = true;
    app.screen = UiScreen::Admin;
    app.jump();
    assert!(outgoing.try_recv().is_err());
    app.screen = UiScreen::Playing;
    app.jump();
    assert_eq!(outgoing.try_recv().unwrap(), ClientMessage::Jump);
    app.flight.flying = true;
    app.jump();
    assert!(outgoing.try_recv().is_err());
}

pub(crate) fn exercise_player_flight(address: &str, path: PathBuf) {
    let profile = 0x5c71;
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
    while app.actions.epoch == 0 || !inventory {
        inventory |= consume(&mut app, deadline);
    }
    assert_eq!(app.position, Vec3::new(0.5, 85.0, 0.5));
    app.admin_enabled = true;
    toggle(&mut app, false, deadline);
    app.screen = UiScreen::Playing;
    app.grabbed = true;
    app.keys.forward = true;
    app.keys.up = true;
    app.yaw = 0.0;
    app.move_player(0.01);
    while !app.unacked.is_empty() {
        consume(&mut app, deadline);
    }
    assert!((app.position.x - 0.58).abs() < 0.001);
    assert_eq!(
        app.position.y, 85.0,
        "held ascent must not fly in walking mode"
    );
    app.keys = Default::default();
    app.jump();
    while app.position.y == 85.0 {
        consume(&mut app, deadline);
    }
    let mut peak = app.position.y;
    while app.position.y > 85.0 {
        peak = peak.max(app.position.y);
        consume(&mut app, deadline);
    }
    assert!(peak > 86.0 && peak < 86.7, "bounded server jump: {peak}");
    assert_eq!(app.position.y, 85.0);

    let action_id = app.actions.allocate().unwrap();
    assert!(app.network.send(ClientMessage::Edit {
        action_id,
        x: 0,
        y: 84,
        z: 0,
        block: crate::world::AIR,
        slot: 0
    }));
    while app.position.y > 84.8 {
        consume(&mut app, deadline);
    }
    toggle(&mut app, true, deadline);
    let stopped_y = app.position.y;
    app.screen = UiScreen::Playing;
    app.grabbed = true;
    app.keys.up = true;
    app.move_player(0.01);
    while !app.unacked.is_empty() {
        consume(&mut app, deadline);
    }
    assert!(
        (app.position.y - (stopped_y + 0.08)).abs() < 0.001,
        "flying must cancel falling velocity and restore ascent"
    );
    app.keys = Default::default();
    toggle(&mut app, false, deadline);
    while app.position.y != 80.0 {
        consume(&mut app, deadline);
    }
    assert_eq!(app.position.x, 0.58);
    app.retire_session();
    assert!(app.flight.flying && app.flight.pending.is_none());
}

fn toggle(app: &mut ClientApp, flying: bool, deadline: Instant) {
    app.screen = UiScreen::Admin;
    app.activate_control(None, UiControl::AdminFlying);
    assert_eq!(app.flight.pending, Some(flying));
    while app.flight.pending.is_some() {
        consume(app, deadline);
    }
    assert_eq!(app.flight.flying, flying);
}

fn consume(app: &mut ClientApp, deadline: Instant) -> bool {
    let incoming = app
        .network
        .incoming
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("flight client deadline");
    let inventory = match incoming {
        Incoming::Message(message) => {
            let inventory = matches!(&*message, ServerMessage::Inventory { .. });
            app.accept(*message);
            inventory
        }
        Incoming::Closed(reason) => panic!("{reason}"),
    };
    assert!(!app.disconnected, "{:?}", app.failure);
    inventory
}
