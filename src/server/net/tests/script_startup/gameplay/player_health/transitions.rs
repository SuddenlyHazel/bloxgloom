//! Ordered life transitions retain their position effects under one WAL receipt.
use super::*;

#[test]
fn player_health_chained_death_respawn_death_preserves_position_and_checkpoint() {
    const POSITION: [f32; 3] = [4.5, 80., 0.5];
    let fixture = fixture();
    std::fs::write(
        fixture.0.join("packages/demo/action.luau"),
        r#"return function(c,e)
            local me=c.player_by_profile(c.player_profile)
            local health=c.player_health(me.session)
            c.damage_player(me.session,health.revision,1000000,'demo:first')
            c.respawn_player(me.session,health.revision,4.5,80,0.5)
            c.damage_player(me.session,health.revision,1000000,'demo:second')
        end"#,
    )
    .unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/policy.luau"),
        r#"return function(e)
            if e.cause=='demo:second' then
                assert(e.player.position[1]==4.5 and e.player.position[2]==80
                    and e.player.position[3]==0.5,'damage policy saw pre-respawn position')
            end
            return e.amount
        end"#,
    )
    .unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/hooks.luau"),
        r#"return function(c,e)
            if e.kind=='died' then
                assert(c.give('player',{item='bloxgloom:stick',count=1}))
                if e.cause=='demo:second' then
                    assert(e.player.position[1]==4.5 and e.player.position[2]==80
                        and e.player.position[3]==0.5,'death hook saw pre-respawn position')
                    c.set_block(2,80,0,'bloxgloom:sand')
                end
            end
        end"#,
    )
    .unwrap();
    let mut initial = state(&fixture);
    for y in 80..=82 {
        initial.world.edit(4, y, 0, AIR).unwrap();
    }
    initial.world.edit(4, 79, 0, crate::world::STONE).unwrap();
    initial.world.edit(2, 80, 0, AIR).unwrap();
    initial
        .position_store
        .save(PROFILE, [0.5, 80., 0.5])
        .unwrap();
    let catalog = initial.world.catalog_arc();
    serve(initial, |address| {
        let mut peer = Peer::connect(address, catalog);
        let request = peer.request(0);
        peer.write(&request);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut accepted = false;
        let mut final_health = None;
        let mut teleports = Vec::new();
        // Publication precedes the action result, but use an explicit network
        // barrier to consume every ordered reset belonging to this receipt.
        loop {
            match peer.read(deadline) {
                ServerMessage::PlayerHealth { health, .. } if health.revision != 0 => {
                    assert_eq!((health.current, health.life, health.revision), (0, 4, 1));
                    assert!(!health.alive);
                    assert!(final_health.replace(health).is_none());
                }
                ServerMessage::PlayerTeleport { position, .. } => teleports.push(position),
                ServerMessage::ActionResult {
                    action_id: id,
                    accepted: result,
                    reason,
                } if id == action_id(&request) => {
                    assert!(result, "chained health transaction rejected: {reason}");
                    accepted = true;
                    peer.write(&ClientMessage::Ping { nonce: 104 });
                }
                ServerMessage::Pong { nonce: 104 } => break,
                _ => {}
            }
        }
        assert!(accepted);
        assert!(final_health.is_some());
        assert_eq!(teleports.last(), Some(&POSITION));
        assert!(teleports.iter().all(|position| *position == POSITION));
        assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 2);
        let store =
            crate::server::position_store::PositionStore::new(&fixture.0.join("save")).unwrap();
        assert_eq!(store.load_with_life(PROFILE).unwrap(), Some((POSITION, 3)));

        assert!(send(&mut peer, &request, None).0, "receipt replay failed");
        peer.write(&ClientMessage::Ping { nonce: 105 });
        loop {
            match peer.read(deadline) {
                ServerMessage::PlayerHealth { .. } | ServerMessage::PlayerTeleport { .. } => {
                    panic!("receipt replay repeated health or teleport publication")
                }
                ServerMessage::Pong { nonce: 105 } => break,
                _ => {}
            }
        }
        assert_eq!(peer.inventory.slots[0].as_ref().unwrap().count, 2);
    });

    let mut reopened = Box::new(fixture.open().unwrap());
    let health = crate::server::players::health::view(&reopened.system_runtime, PROFILE).unwrap();
    assert_eq!((health.current, health.life, health.revision), (0, 4, 1));
    assert_eq!(reopened.world.get_block(2, 80, 0).unwrap(), SAND);
    assert_eq!(
        reopened.position_store.load_with_life(PROFILE).unwrap(),
        Some((POSITION, 3))
    );
    let catalog = reopened.world.catalog_arc();
    serve(reopened, |address| {
        // Read the actual admission Position rather than discarding it in
        // Peer::connect: reconnect must not surprise the client with a repair.
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut stream,
            &ClientMessage::Hello {
                name: "chained-health-reconnect".into(),
                profile: PROFILE,
                content_fingerprint: catalog.fingerprint(),
            },
        )
        .unwrap();
        let (fingerprint, _) = receive_content_manifest(&mut stream);
        assert_eq!(fingerprint, catalog.fingerprint());
        protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint }).unwrap();
        let mut joined_position = None;
        loop {
            match protocol::read_server_with_catalog(&mut stream, &catalog).unwrap() {
                ServerMessage::Position { x, y, z, .. } => joined_position = Some([x, y, z]),
                ServerMessage::PlayerTeleport { .. } => {
                    panic!("reconnect repaired a committed pose")
                }
                ServerMessage::PlayerHealth { health: value, .. } => {
                    assert_eq!(value, health);
                    assert_eq!(joined_position, Some(POSITION));
                    protocol::write_client(&mut stream, &ClientMessage::Ping { nonce: 106 })
                        .unwrap();
                }
                ServerMessage::Pong { nonce: 106 } => break,
                _ => {}
            }
        }
    });
}
