//! Health transition audio shares the health/inventory receipt and bus contract.
use super::*;
use bloxgloom_host_api::sound::{Bus, Event, Kind};

fn result(peer: &mut Peer, request: &ClientMessage) -> (bool, Vec<Event>) {
    peer.write(request);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut sounds = Vec::new();
    loop {
        match peer.read(deadline) {
            ServerMessage::Sounds { events, .. } => sounds.extend(events),
            ServerMessage::ActionResult {
                action_id: id,
                accepted,
                ..
            } if id == action_id(request) => return (accepted, sounds),
            _ => {}
        }
    }
}

#[test]
fn player_health_hook_bus_audio_commits_once_and_invalid_routing_rolls_back_death() {
    for invalid in [true, false] {
        let fixture = fixture();
        let bus = if invalid { "unknown" } else { "ui" };
        std::fs::write(
            fixture.0.join("packages/demo/hooks.luau"),
            format!(
                r#"return function(c,e)
                    assert(e.kind=='died')
                    assert(c.give('player',{{item='bloxgloom:stick',count=1}}))
                    pcall(function()
                        c.sound{{kind='play',voice='death-tone',clip='bloxgloom:pickup',
                            bus='{bus}',position=e.player.position}}
                    end)
                end"#
            ),
        )
        .unwrap();
        let initial = state(&fixture);
        let catalog = initial.world.catalog_arc();
        serve(initial, |address| {
            let mut peer = Peer::connect(address, catalog);
            let request = peer.request(3);
            let (accepted, sounds) = result(&mut peer, &request);
            assert_eq!(accepted, !invalid);
            if invalid {
                assert!(sounds.is_empty(), "rejected death published audio");
            } else {
                let tones: Vec<_> = sounds
                    .iter()
                    .filter(|event| event.owner == "demo" && event.voice == "death-tone")
                    .collect();
                assert_eq!(tones.len(), 1);
                assert!(matches!(
                    &tones[0].kind,
                    Kind::Play { bus: Bus::Ui, clip, .. } if clip == "bloxgloom:pickup"
                ));
            }
            let (replayed, sounds) = result(&mut peer, &request);
            assert_eq!(replayed, accepted);
            assert!(sounds.is_empty(), "receipt replay repeated death audio");
        });
        let recovered = fixture.open().unwrap();
        let health =
            crate::server::players::health::view(&recovered.system_runtime, PROFILE).unwrap();
        assert_eq!(health.alive, invalid);
        let inventory = recovered.inventory_store.load(PROFILE).unwrap();
        if invalid {
            assert!(inventory.slots.iter().all(Option::is_none));
        } else {
            assert_eq!(inventory.slots[0].as_ref().unwrap().count, 1);
        }
    }
}
