//! Exercise the shipped player demos, not hand-written equivalents.
use super::*;
use gameplay::{Peer, serve};

fn open(fixture: &Fixture, name: &str) -> Box<State> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
        .join("packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&root)
        .unwrap();
    Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    )
}

fn notices(peer: &mut Peer, texts: &[&str]) {
    let mut pending = texts
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !pending.is_empty() {
        if let ServerMessage::PlayerNotice { text: actual, .. } = peer.read(deadline) {
            pending.remove(actual.as_str());
        }
    }
}

#[test]
fn shipped_region_demo_reports_both_edges_and_moderates_chat_over_tcp() {
    let fixture = Fixture::new();
    let mut state = open(&fixture, "player-world");
    state.spawn_anchor = [15.5, 80.0, 0.5];
    state
        .position_store
        .save(0x5c71, [15.5, 80.0, 0.5])
        .unwrap();
    for x in 15..=17 {
        for y in 79..=82 {
            state
                .world
                .edit(
                    x,
                    y,
                    0,
                    if y == 79 {
                        crate::world::STONE
                    } else {
                        crate::world::AIR
                    },
                )
                .unwrap();
        }
    }
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        notices(&mut peer, &["Entered demo:spawn", "Entered chunk 0,5,0"]);
        peer.write(&ClientMessage::Move {
            seq: 1,
            dx: 0.6,
            dy: 0.0,
            dz: 0.0,
        });
        notices(
            &mut peer,
            &["Left demo:spawn", "Left chunk 0,5,0; entered chunk 1,5,0"],
        );
        peer.write(&ClientMessage::Move {
            seq: 2,
            dx: -0.6,
            dy: 0.0,
            dz: 0.0,
        });
        notices(
            &mut peer,
            &[
                "Entered demo:spawn",
                "Left chunk 1,5,0; entered chunk 0,5,0",
            ],
        );
        peer.write(&ClientMessage::Chat {
            sequence: 1,
            text: "hello".into(),
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::Chat { message } = peer.read(deadline) {
                assert_eq!(message.text, "hello");
                break;
            }
        }
        peer.write(&ClientMessage::Chat {
            sequence: 2,
            text: "spoiler".into(),
        });
        loop {
            if let ServerMessage::ChatRejected { text } = peer.read(deadline) {
                assert_eq!(text, "Please keep this demo spoiler free.");
                break;
            }
        }
        thread::sleep(Duration::from_millis(450));
        peer.write(&ClientMessage::Ping { nonce: 101 });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match peer.read(deadline) {
                ServerMessage::Pong { nonce: 101 } => break,
                ServerMessage::PlayerNotice { text, .. } => assert!(
                    !text.starts_with("Left chunk"),
                    "stationary chunk tracker repeated: {text}"
                ),
                _ => {}
            }
        }
    });
}

#[test]
fn shipped_modifier_demo_self_commands_apply_and_clear_authoritative_rates() {
    let fixture = Fixture::new();
    let mut state = open(&fixture, "player-modifiers");
    state.admin_profile = Some(0x5c71);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, catalog);
        for (mode, speed, notice) in [
            (1, 1.25, "Saved walking speed boost: +25%"),
            (2, 0.625, "Temporary speed multiplier: 50% for 600 ticks"),
            (3, 1.0, "Movement effects cleared"),
        ] {
            let action_id = peer.next_id();
            peer.write(&ClientMessage::EntityInteract {
                action_id,
                target: [0; 3],
                payload: bloxgloom_host_api::actions::Request {
                    key: "pace:self".into(),
                    version: 1,
                    slot: 0,
                    inventory_revision: peer.inventory.revision,
                    entity: 0,
                    entity_revision: 0,
                    arguments: vec![mode],
                }
                .encode()
                .unwrap(),
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut accepted, mut updated, mut notified) = (false, false, false);
            while !accepted || !updated || !notified {
                match peer.read(deadline) {
                    ServerMessage::ActionResult {
                        action_id: id,
                        accepted: result,
                        reason,
                    } if id == action_id => {
                        assert!(result, "{reason}");
                        accepted = true;
                    }
                    ServerMessage::PlayerModifiers {
                        movement,
                        session,
                        reset,
                        ..
                    } if movement.speed == speed => {
                        updated = true;
                        peer.write(&ClientMessage::MovementReady {
                            session,
                            reset,
                            next_seq: u64::from(mode),
                        });
                    }
                    ServerMessage::PlayerNotice { text, .. } if text == notice => notified = true,
                    _ => {}
                }
            }
        }
    });
}
