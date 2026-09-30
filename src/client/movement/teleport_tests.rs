//! Actual transport and ClientApp prediction retirement through a real listener.
use crate::client::*;

pub(crate) fn exercise_player_teleport(address: &str, path: PathBuf) {
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
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut inventory = false;
    while app.actions.epoch == 0 || !inventory {
        inventory |= consume(&mut app, deadline);
    }
    let action_id = app.actions.allocate().unwrap();
    let request = ClientMessage::EntityInteract {
        action_id,
        target: [0; 3],
        payload: bloxgloom_host_api::actions::Request {
            key: "demo:shift".into(),
            version: 1,
            slot: 0,
            inventory_revision: app.inventory.revision,
            entity: 0,
            entity_revision: 0,
            arguments: vec![0],
        }
        .encode()
        .unwrap(),
    };
    app.next_seq = 6;
    app.unacked.push_back((5, Vec3::new(50.0, 0.0, 0.0)));
    assert!(app.network.send(request));
    while app.movement_reset == 0 {
        consume(&mut app, deadline);
    }
    assert!(!app.disconnected, "{:?}", app.failure);
    assert_eq!(app.position, Vec3::new(1200.5, 300.0, 0.5));
    assert!(
        app.unacked.is_empty(),
        "old input was replayed at destination"
    );
    app.screen = UiScreen::Playing;
    app.grabbed = true;
    app.keys.forward = true;
    app.yaw = 0.0;
    let displacement = app.catalog.player_rules().motion().intent_blocks_per_second * 0.01;
    app.move_player(0.01);
    assert_eq!(app.next_seq, 7);
    assert_eq!(app.unacked.len(), 1);
    while !app.unacked.is_empty() {
        consume(&mut app, deadline);
    }
    assert!(
        (app.position.x - (1200.5 + displacement)).abs() < 0.001,
        "unexpected position {:?}",
        app.position
    );
    assert_eq!((app.position.y, app.position.z), (300.0, 0.5));
    app.retire_session();
    assert_eq!(app.movement_reset, 0);
}
fn consume(app: &mut ClientApp, deadline: Instant) -> bool {
    let incoming = app
        .network
        .incoming
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .expect("teleport client deadline");
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

#[test]
fn teleport_client_reset_is_session_scoped_idempotent_and_discards_prediction() {
    let (mut network, receiver) = Network::capture_outgoing_for_test();
    network.profile = 1;
    let mut app = ClientApp::new(
        network,
        Config::default(),
        std::env::temp_dir().join("unused-teleport-reset-config"),
    );
    app.accept(ServerMessage::ActionSession {
        epoch: 5,
        next_seq: 1,
        acked_seq: 0,
    });
    app.next_seq = 9;
    app.unacked.push_back((8, Vec3::new(5.0, 0.0, 0.0)));
    app.accept(ServerMessage::PlayerTeleport {
        profile: 1,
        session: 5,
        reset: 1,
        position: [33.5, 300.0, 0.5],
    });
    assert_eq!(
        receiver.try_recv().unwrap(),
        ClientMessage::MovementReady {
            session: 5,
            reset: 1,
            next_seq: 9
        }
    );
    assert!(app.unacked.is_empty() && !app.disconnected);
    app.accept(ServerMessage::PlayerTeleport {
        profile: 1,
        session: 5,
        reset: 1,
        position: [49.5, 300.0, 0.5],
    });
    assert_eq!(app.position, Vec3::new(33.5, 300.0, 0.5));
    assert!(
        receiver.try_recv().is_err(),
        "duplicate reset repeated acknowledgment"
    );
    app.accept(ServerMessage::PlayerTeleport {
        profile: 1,
        session: 4,
        reset: 2,
        position: [49.5, 300.0, 0.5],
    });
    assert!(app.disconnected && app.movement_reset == 0);
    app.accept(ServerMessage::PlayerTeleport {
        profile: 1,
        session: 5,
        reset: 3,
        position: [49.5, 300.0, 0.5],
    });
    assert_eq!(app.position, Vec3::new(33.5, 300.0, 0.5));
}
