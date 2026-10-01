//! Exercises actual listener, bundle WAV preparation, commit publication and retries.
use super::*;
fn receive_result(
    peer: &mut Peer,
    expected: u128,
) -> (bool, Vec<bloxgloom_host_api::sound::Event>) {
    let mut sounds = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match peer.read(deadline) {
            ServerMessage::Sounds { events, .. } => sounds.extend(events),
            ServerMessage::ActionResult {
                action_id,
                accepted,
                reason,
            } if action_id == expected => {
                if !accepted {
                    eprintln!("rejected sound test action: {reason}");
                }
                return (accepted, sounds);
            }
            _ => {}
        }
    }
}
#[test]
fn packaged_audio_commits_once_and_caught_invalid_audio_rolls_back() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.action(REGISTER,r#"return function(c,e)
        c.sound{kind='play',voice='ping',clip='demo:beep',position={0.5,80,0.5}}
        c.set_block(2,80,0,if e.arguments:byte(1)==1 then 'bloxgloom:sand' else 'bloxgloom:glowstone')
        if e.arguments:byte(1)==1 then pcall(function() c.sound{kind='play',voice='bad',clip='demo:missing',position={0,80,0}} end) end
    end"#);
    let dir = fixture.0.join("packages/demo");
    std::fs::create_dir_all(dir.join("server")).unwrap();
    std::fs::copy(dir.join("main.luau"), dir.join("server/main.luau")).unwrap();
    std::fs::copy(dir.join("action.luau"), dir.join("server/action.luau")).unwrap();
    let manifest = std::fs::read_to_string(dir.join("package.txt"))
        .unwrap()
        .replace("format 1", "format 2")
        .replace(
            "module main main.luau",
            "module server main server/main.luau",
        )
        .replace(
            "module action action.luau",
            "module server action server/action.luau",
        );
    std::fs::write(
        dir.join("package.txt"),
        format!("{manifest}\nasset sound beep assets/sounds/beep.wav\n"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("assets/sounds")).unwrap();
    std::fs::write(
        dir.join("assets/sounds/beep.wav"),
        include_bytes!("../../../../../../assets/sounds/pickup.wav"),
    )
    .unwrap();
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80.0, 0.5];
    state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
    for y in 80..83 {
        state.world.edit(0, y, 0, AIR).unwrap();
    }
    let catalog = state.world.catalog_arc();
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(
        catalog.item_by_key("bloxgloom:stick").unwrap(),
        1,
    ));
    state.inventory_store.save(PROFILE, &inv).unwrap();
    serve(state, |addr| {
        let mut peer = Peer::connect(addr, Arc::clone(&catalog));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let key = match peer.read(deadline) {
                ServerMessage::Chunk(chunk) => Some(chunk.key),
                ServerMessage::WorldSnapshotStart(start) => Some(start.chunk.key),
                _ => None,
            };
            if key == Some(crate::world::ChunkKey { x: 0, y: 5, z: 0 }) {
                break;
            }
        }
        let request = peer.request(0);
        let ClientMessage::EntityInteract { action_id, .. } = request else {
            unreachable!()
        };
        peer.write(&request);
        let (accepted, sounds) = receive_result(&mut peer, action_id);
        assert!(accepted);
        assert_eq!(sounds.iter().filter(|e|matches!(&e.kind,bloxgloom_host_api::sound::Kind::Play{clip,..} if clip=="demo:beep")).count(),1);
        peer.write(&request);
        let (accepted, sounds) = receive_result(&mut peer, action_id);
        assert!(accepted);
        assert!(sounds.is_empty(), "receipt retry must not replay sound");
        let bad = peer.request(1);
        let ClientMessage::EntityInteract { action_id, .. } = bad else {
            unreachable!()
        };
        peer.write(&bad);
        let (accepted, sounds) = receive_result(&mut peer, action_id);
        assert!(!accepted);
        assert!(sounds.is_empty());
    });
    let mut reopened = fixture.open().unwrap();
    assert_eq!(reopened.world.get_block(2, 80, 0).unwrap(), GLOWSTONE);
}
#[test]
fn invalid_packaged_wav_never_installs_a_partial_catalog() {
    let fixture = Fixture::new();
    fixture.action(REGISTER, "return function() end");
    let dir = fixture.0.join("packages/demo");
    std::fs::create_dir_all(dir.join("server")).unwrap();
    std::fs::copy(dir.join("main.luau"), dir.join("server/main.luau")).unwrap();
    std::fs::copy(dir.join("action.luau"), dir.join("server/action.luau")).unwrap();
    let manifest = std::fs::read_to_string(dir.join("package.txt"))
        .unwrap()
        .replace("format 1", "format 2")
        .replace(
            "module main main.luau",
            "module server main server/main.luau",
        )
        .replace(
            "module action action.luau",
            "module server action server/action.luau",
        );
    std::fs::write(
        dir.join("package.txt"),
        format!("{manifest}\nasset sound beep assets/sounds/beep.wav\n"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("assets/sounds")).unwrap();
    std::fs::write(dir.join("assets/sounds/beep.wav"), b"not a WAV").unwrap();
    let error = fixture
        .open()
        .err()
        .expect("invalid WAV must reject startup");
    assert!(error.to_string().contains("demo:beep"), "{error}");
}

#[test]
fn audio_timer_fixture_places_completes_and_reconstructs_its_replica_loop() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/audio-machine/packages");
    let startup = ServerStartup::new(Arc::new(Catalog::builtins()))
        .with_local_packages(&root)
        .unwrap();
    let mut state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 2, startup).unwrap(),
    );
    state.spawn_anchor = [0.5, 80.0, 0.5];
    for x in -1..=3 {
        for z in -1..=1 {
            for y in 79..=83 {
                state
                    .world
                    .edit(x, y, z, if y == 79 { crate::world::STONE } else { AIR })
                    .unwrap();
            }
        }
    }
    let catalog = state.world.catalog_arc();
    let machine = catalog.state_by_key("audio:machine").unwrap();
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(catalog.item_by_key("audio:machine").unwrap(), 1));
    state.inventory_store.save(PROFILE, &inv).unwrap();
    let bundle = Arc::clone(state.client_bundle.as_ref().unwrap());
    let prepared = crate::client::startup::prepare(Arc::clone(&bundle)).unwrap();
    let mut visual = crate::client::presentation::VisualSession::with_parameters(
        Arc::clone(prepared.replica.as_ref().unwrap()),
        Default::default(),
    )
    .unwrap();
    visual.entities(
        vec![crate::client::presentation::EntityView {
            id: 123,
            key: "audio:timer".into(),
            position: [2.5, 80.5, 0.5],
            revision: 1,
            motion_revision: 1,
            public: vec![1],
        }],
        1,
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let loop_event = loop {
        visual.poll();
        let events = visual.take_sounds();
        if !events.is_empty() {
            break events;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    };
    assert!(
        matches!(&loop_event[0].kind,bloxgloom_host_api::sound::Kind::Play{clip,entity:Some(123),looping:true,..} if clip=="audio:motor")
    );
    visual.entities(
        vec![crate::client::presentation::EntityView {
            id: 123,
            key: "audio:timer".into(),
            position: [2.5, 80.5, 0.5],
            revision: 2,
            motion_revision: 1,
            public: vec![0],
        }],
        1,
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        visual.poll();
        let events = visual.take_sounds();
        if !events.is_empty() {
            assert!(matches!(
                events[0].kind,
                bloxgloom_host_api::sound::Kind::Stop
            ));
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    serve(state, |addr| {
        let mut peer = Peer::connect(addr, Arc::clone(&catalog));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let key = match peer.read(deadline) {
                ServerMessage::Chunk(chunk) => Some(chunk.key),
                ServerMessage::WorldSnapshotStart(start) => Some(start.chunk.key),
                _ => None,
            };
            if key == Some(crate::world::ChunkKey { x: 0, y: 5, z: 0 }) {
                break;
            }
        }
        let action_id = (u128::from(peer.epoch) << 64) | u128::from(peer.sequence);
        let message = ClientMessage::Edit {
            action_id,
            x: 2,
            y: 80,
            z: 0,
            block: machine,
            slot: 0,
        };
        peer.write(&message);
        let (accepted, sounds) = receive_result(&mut peer, action_id);
        assert!(accepted);
        assert!(sounds.iter().any(|e|matches!(&e.kind,bloxgloom_host_api::sound::Kind::Play{clip,..} if clip=="audio:start")));
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut completed = 0;
        loop {
            if let ServerMessage::Sounds { events, .. } = peer.read(deadline) {
                completed += events.iter().filter(|event| matches!(&event.kind, bloxgloom_host_api::sound::Kind::Play { clip, .. } if clip == "audio:complete")).count();
                if completed != 0 {
                    break;
                }
            }
        }
        assert_eq!(completed, 1);
    });
}
