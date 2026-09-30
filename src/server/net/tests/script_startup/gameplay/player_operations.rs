//! Session effects must share authorization, rollback and receipt deduplication.
use super::*;

fn fixture(authority: bool) -> Fixture {
    let fixture = Fixture::new();
    fixture.action("return function(h) h.register_action('demo:shift',1,'Session effect','empty',nil,'demo:action') end",r#"
        return function(c,e)
            local me=c.player_by_profile(c.player_profile)
            local mode=string.byte(e.arguments,1)
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if mode==3 then c.kick_player(me.session,'Test removal')
            else
                c.message_player(me.session,'Message '..tostring(mode))
                if mode==1 then error('reject after message') end
                if mode==2 then pcall(function() c.kick_player(me.session,'bad\nreason') end) end
            end
        end
    "#);
    if authority {
        let path = fixture.0.join("packages/demo/package.txt");
        let manifest = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path, format!("{manifest}requires bloxgloom:players/v1\n")).unwrap();
    }
    fixture
}

fn send_collect(peer: &mut Peer, request: &ClientMessage) -> (bool, Vec<String>) {
    let ClientMessage::EntityInteract { action_id, .. } = request else {
        unreachable!()
    };
    protocol::write_client(&mut peer.stream, request).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut notices = Vec::new();
    loop {
        match peer.read(deadline) {
            ServerMessage::PlayerNotice {
                profile,
                session,
                kicked,
                text,
            } => {
                assert_eq!((profile, session), (PROFILE, peer.epoch));
                assert!(!kicked);
                notices.push(text);
            }
            ServerMessage::ActionResult {
                action_id: id,
                accepted,
                ..
            } if id == *action_id => return (accepted, notices),
            _ => {}
        }
    }
}

#[test]
fn player_notices_require_package_authority_and_share_rollback_and_receipts() {
    for authority in [false, true] {
        let fixture = fixture(authority);
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            let mut peer = Peer::connect(address, catalog);
            for mode in [1, 2] {
                let request = peer.request(mode);
                assert_eq!(send_collect(&mut peer, &request), (false, vec![]));
            }
            let request = peer.request(0);
            let result = send_collect(&mut peer, &request);
            if authority {
                assert_eq!(result, (true, vec!["Message 0".into()]));
                assert_eq!(
                    send_collect(&mut peer, &request),
                    (true, vec![]),
                    "receipt replay duplicated a notice"
                );
                peer.inventory_at(1);
            } else {
                assert_eq!(result, (false, vec![]));
                assert!(peer.inventory.slots.iter().all(Option::is_none));
            }
        });
        let state = fixture.open().unwrap();
        assert_eq!(
            state.inventory_store.load(PROFILE).unwrap().slots[0]
                .as_ref()
                .map(|s| s.count),
            authority.then_some(1)
        );
    }
}

#[test]
fn committed_kick_closes_real_listener_session_and_reconnect_has_new_epoch() {
    let fixture = fixture(true);
    let state = Box::new(fixture.open().unwrap());
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, Arc::clone(&catalog));
        let epoch = peer.epoch;
        let request = peer.request(3);
        protocol::write_client(&mut peer.stream, &request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let ServerMessage::PlayerNotice {
                profile,
                session,
                kicked,
                text,
            } = peer.read(deadline)
            {
                assert_eq!(
                    (profile, session, kicked, text),
                    (PROFILE, epoch, true, "Test removal".into())
                );
                break;
            }
        }
        loop {
            match protocol::read_server_with_catalog(&mut peer.stream, &peer.catalog) {
                Ok(_) => assert!(Instant::now() < deadline, "kicked connection stayed open"),
                Err(error) => {
                    assert!(matches!(
                        error.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                    ));
                    break;
                }
            }
        }
        let mut replacement = Peer::connect(address, catalog);
        assert!(replacement.epoch > epoch);
        assert!(
            !replacement.send(&request).0,
            "old session kick replay accepted"
        );
        let next = replacement.request(0);
        assert_eq!(
            send_collect(&mut replacement, &next),
            (true, vec!["Message 0".into()])
        );
        replacement.inventory_at(2);
    });
}

#[test]
fn joined_lifecycle_can_send_notice_but_admission_cannot_stage_session_effects() {
    for joining in [false, true] {
        let fixture = Fixture::new();
        fixture.package("demo","requires bloxgloom:players/v1\nmodule player player.luau",
            "return function(h) h.register_player_lifecycle('demo:progress',1,64,'','demo:player') end");
        std::fs::write(
            fixture.0.join("packages/demo/player.luau"),
            format!(
                r#"
            return function(c,e)
                if e.kind=='{}' then c.message_player(e.player.session,'Welcome') end
            end
        "#,
                if joining {
                    "PlayerJoining"
                } else {
                    "PlayerJoined"
                }
            ),
        )
        .unwrap();
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        serve(state, |address| {
            if joining {
                let mut stream = TcpStream::connect(address).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                protocol::write_client(
                    &mut stream,
                    &ClientMessage::Hello {
                        name: "rejected".into(),
                        profile: PROFILE,
                        content_fingerprint: catalog.fingerprint(),
                    },
                )
                .unwrap();
                let (fingerprint, _) = receive_content_manifest(&mut stream);
                protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint })
                    .unwrap();
                assert!(
                    protocol::read_server(&mut stream).is_err(),
                    "admission effect escaped"
                );
            } else {
                let mut stream = TcpStream::connect(address).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                protocol::write_client(
                    &mut stream,
                    &ClientMessage::Hello {
                        name: "joined".into(),
                        profile: PROFILE,
                        content_fingerprint: catalog.fingerprint(),
                    },
                )
                .unwrap();
                let (fingerprint, _) = receive_content_manifest(&mut stream);
                protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    assert!(Instant::now() < deadline);
                    if let ServerMessage::PlayerNotice { text, kicked, .. } =
                        protocol::read_server_with_catalog(&mut stream, &catalog).unwrap()
                    {
                        assert_eq!(text, "Welcome");
                        assert!(!kicked);
                        break;
                    }
                }
            }
        });
    }
}
