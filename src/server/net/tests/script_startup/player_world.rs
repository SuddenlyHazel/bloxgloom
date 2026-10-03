//! Production nonblocking transport: authoritative regions and moderated chat.
use super::*;
use gameplay::{Peer, serve};
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.package("demo","requires bloxgloom:players/v1\nrequires bloxgloom:actions/v1\nmodule players players.luau\nmodule chat chat.luau\nmodule action action.luau",r#"
    return function(h)
        h.register_player_lifecycle('demo:players',1,32,'','demo:players')
        h.register_region('demo:west',{-1,70,-1},{1,100,1},'demo:players')
        h.register_chat_hook('demo:chat',1,'demo:chat')
        h.register_action('demo:teleport',1,'Teleport','empty',nil,'demo:action')
    end"#);
    std::fs::write(
        fixture.0.join("packages/demo/players.luau"),
        r#"
    return function(c,e)
        if e.kind=='RegionEntered' or e.kind=='RegionLeft' then
            assert(e.region=='demo:west')
            assert(c.give('player',{item='bloxgloom:stick',count=1}))
            if c.player_by_session(e.player.session) then c.message_player(e.player.session,e.kind .. ':' .. e.region) end
        end
    end"#,
    )
    .unwrap();
    std::fs::write(
        fixture.0.join("packages/demo/chat.luau"),
        r#"
    return function(e)
        assert(e.identity_trust=='claimed_profile' and e.sender.online)
        assert(e.set_block==nil and e.give==nil)
        assert(not pcall(function() e.sender.name='forged' end))
        if e.text=='deny' then return {allow=false,reason='Denied by package'} end
        if e.text=='invalid' then return nil end
        if e.text=='self' then return {allow=true,text='private',recipients={e.sender}} end
        return {allow=true,text='[' .. e.text .. ']'}
    end"#,
    )
    .unwrap();
    std::fs::write(fixture.0.join("packages/demo/action.luau"),"return function(c) local me=c.player_by_profile(c.player_profile); c.teleport_player(me.session,4.5,80,0.5) end").unwrap();
    fixture
}
fn state(fixture: &Fixture) -> Box<State> {
    let mut state = Box::new(fixture.open().unwrap());
    state.spawn_anchor = [0.5, 80., 0.5];
    for x in -1..=5 {
        for y in 80..=82 {
            state.world.edit(x, y, 0, crate::world::AIR).unwrap();
        }
        state.world.edit(x, 79, 0, crate::world::STONE).unwrap();
    }
    state
}
fn notice(peer: &mut Peer, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let ServerMessage::PlayerNotice { text, .. } = peer.read(deadline)
            && text == expected
        {
            break;
        }
    }
}
fn chat(peer: &mut Peer) -> bloxgloom_host_api::chat::Message {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let ServerMessage::Chat { message } = peer.read(deadline) {
            return message;
        }
    }
}
fn rejected(peer: &mut Peer) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let ServerMessage::ChatRejected { text } = peer.read(deadline) {
            return text;
        }
    }
}
#[test]
fn package_regions_join_movement_teleport_and_disconnect_use_transactional_hooks() {
    let fixture = fixture();
    let state = state(&fixture);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut peer = Peer::connect(address, Arc::clone(&catalog));
        notice(&mut peer, "RegionEntered:demo:west");
        peer.write(&ClientMessage::Move {
            seq: 1,
            dx: 0.6,
            dy: 0.,
            dz: 0.,
        });
        notice(&mut peer, "RegionLeft:demo:west");
        peer.write(&ClientMessage::Move {
            seq: 2,
            dx: -0.6,
            dy: 0.,
            dz: 0.,
        });
        notice(&mut peer, "RegionEntered:demo:west");
        let deadline = Instant::now() + Duration::from_secs(10);
        while peer.inventory.slots[0].as_ref().map(|stack| stack.count) != Some(3) {
            peer.read(deadline);
        }
        let action_id = peer.next_id();
        let request = ClientMessage::EntityInteract {
            action_id,
            target: [0; 3],
            payload: bloxgloom_host_api::actions::Request {
                key: "demo:teleport".into(),
                version: 1,
                slot: 1,
                inventory_revision: peer.inventory.revision,
                entity: 0,
                entity_revision: 0,
                arguments: Vec::new(),
            }
            .encode()
            .unwrap(),
        };
        let (accepted, reason) = peer.send(&request);
        assert!(accepted, "{reason}");
        notice(&mut peer, "RegionLeft:demo:west");
        let mut other = Peer::connect_profile(address, Arc::clone(&catalog), 0x777);
        notice(&mut other, "RegionEntered:demo:west");
        other.stream.shutdown(Shutdown::Both).unwrap();
        // A remaining socket acts as a production tick barrier for the leaving WAL.
        thread::sleep(Duration::from_millis(500));
    });
    let reopened = fixture.open().unwrap();
    let sticks = |profile| {
        reopened
            .inventory_store
            .load(profile)
            .unwrap()
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.item == catalog.item_by_key("bloxgloom:stick").unwrap())
            .map(|stack| u32::from(stack.count))
            .sum::<u32>()
    };
    assert_eq!(sticks(0x5c71), 4);
    assert_eq!(sticks(0x777), 2);
}
#[test]
fn package_chat_formats_routes_denies_throttles_and_fences_replays() {
    let fixture = fixture();
    let state = state(&fixture);
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut sender = Peer::connect(address, Arc::clone(&catalog));
        notice(&mut sender, "RegionEntered:demo:west");
        let mut other = Peer::connect_profile(address, catalog, 0x778);
        notice(&mut other, "RegionEntered:demo:west");
        sender.write(&ClientMessage::Chat {
            sequence: 1,
            text: "héllo 雨".into(),
        });
        let first = chat(&mut sender);
        assert_eq!(first.profile, 0x5c71);
        assert_eq!(first.name, "luau-action");
        assert_eq!(first.text, "[héllo 雨]");
        assert_eq!(chat(&mut other), first);
        sender.write(&ClientMessage::Chat {
            sequence: 1,
            text: "replay".into(),
        });
        sender.write(&ClientMessage::Chat {
            sequence: 2,
            text: "deny".into(),
        });
        assert_eq!(rejected(&mut sender), "Denied by package");
        sender.write(&ClientMessage::Chat {
            sequence: 3,
            text: "invalid".into(),
        });
        assert!(rejected(&mut sender).contains("invalid moderation"));
        sender.write(&ClientMessage::Chat {
            sequence: 4,
            text: "self".into(),
        });
        assert_eq!(chat(&mut sender).text, "private");
        sender.write(&ClientMessage::Chat {
            sequence: 5,
            text: "too fast".into(),
        });
        assert!(rejected(&mut sender).contains("rate limit"));
        other.write(&ClientMessage::Ping { nonce: 812 });
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match other.read(deadline) {
                ServerMessage::Pong { nonce: 812 } => break,
                ServerMessage::Chat { message } => {
                    panic!("denied, private or replayed chat leaked: {message:?}")
                }
                _ => {}
            }
        }
    });
}
#[test]
fn invalid_region_declarations_fail_before_opening_world() {
    for source in [
        "h.register_region('demo:r',{0,0,0},{0,1,1},'demo:players')",
        "h.register_region('demo:r',{0,0,0},{1,1,1},'demo:missing')",
        "h.register_region('other:r',{0,0,0},{1,1,1},'demo:players')",
    ] {
        let fixture = Fixture::new();
        fixture.package(
            "demo",
            "requires bloxgloom:players/v1",
            &format!("return function(h) {source} end"),
        );
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save/world.meta").exists());
    }
}

struct PausedModeration {
    started: std::sync::mpsc::Sender<()>,
    released: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}
impl bloxgloom_host_api::chat::Moderator for PausedModeration {
    fn moderate(
        &self,
        request: &bloxgloom_host_api::chat::Request,
    ) -> Result<bloxgloom_host_api::chat::Decision, String> {
        if request.text == "slow" {
            let _ = self.started.send(());
            let (lock, wake) = &*self.released;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = wake.wait(released).unwrap();
            }
        }
        Ok(bloxgloom_host_api::chat::Decision::Allow {
            text: request.text.clone(),
            route: bloxgloom_host_api::chat::Route::All,
        })
    }
}
struct PausedExtension(Arc<PausedModeration>);
impl bloxgloom_host_api::Extension for PausedExtension {
    fn register(
        &self,
        registrar: &mut dyn bloxgloom_host_api::Registrar,
    ) -> Result<(), bloxgloom_host_api::RegistrationError> {
        registrar.chat_hook(bloxgloom_host_api::chat::Registration {
            key: "demo:aaa_pause".into(),
            revision: 1,
            moderator: self.0.clone(),
        })
    }
}
#[test]
fn background_chat_keeps_tcp_responsive_and_discards_departed_sender() {
    let fixture = fixture();
    let (started, wait) = std::sync::mpsc::channel();
    let released = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let startup = fixture
        .startup(Arc::new(Catalog::builtins()))
        .unwrap()
        .with_extension(&PausedExtension(Arc::new(PausedModeration {
            started,
            released: Arc::clone(&released),
        })))
        .unwrap();
    let mut state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 4, startup).unwrap(),
    );
    state.spawn_anchor = [0.5, 80., 0.5];
    for y in 80..=82 {
        state.world.edit(0, y, 0, crate::world::AIR).unwrap();
    }
    state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut sender = Peer::connect(address, Arc::clone(&catalog));
        notice(&mut sender, "RegionEntered:demo:west");
        let mut other = Peer::connect_profile(address, catalog, 0x779);
        notice(&mut other, "RegionEntered:demo:west");
        sender.write(&ClientMessage::Chat {
            sequence: 1,
            text: "slow".into(),
        });
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        other.write(&ClientMessage::Ping { nonce: 991 });
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let ServerMessage::Pong { nonce: 991 } = other.read(deadline) {
                break;
            }
        }
        sender.stream.shutdown(Shutdown::Both).unwrap();
        thread::sleep(Duration::from_millis(150));
        *released.0.lock().unwrap() = true;
        released.1.notify_all();
        other.write(&ClientMessage::Chat {
            sequence: 1,
            text: "fresh".into(),
        });
        assert_eq!(
            chat(&mut other).text,
            "[fresh]",
            "departed sender's pending message was delivered"
        );
    });
}
#[test]
fn moderated_chat_does_not_follow_a_profile_into_a_replacement_session() {
    let fixture = fixture();
    let (started, wait) = std::sync::mpsc::channel();
    let released = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let startup = fixture
        .startup(Arc::new(Catalog::builtins()))
        .unwrap()
        .with_extension(&PausedExtension(Arc::new(PausedModeration {
            started,
            released: Arc::clone(&released),
        })))
        .unwrap();
    let mut state = Box::new(
        crate::server::server_state_with_startup(7, fixture.0.join("save"), 4, startup).unwrap(),
    );
    state.spawn_anchor = [0.5, 80., 0.5];
    for y in 80..=82 {
        state.world.edit(0, y, 0, crate::world::AIR).unwrap();
    }
    state.world.edit(0, 79, 0, crate::world::STONE).unwrap();
    let catalog = state.world.catalog_arc();
    serve(state, |address| {
        let mut sender = Peer::connect(address, Arc::clone(&catalog));
        notice(&mut sender, "RegionEntered:demo:west");
        let mut previous = Peer::connect_profile(address, Arc::clone(&catalog), 0x780);
        notice(&mut previous, "RegionEntered:demo:west");
        sender.write(&ClientMessage::Chat {
            sequence: 1,
            text: "slow".into(),
        });
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        previous.stream.shutdown(Shutdown::Both).unwrap();
        thread::sleep(Duration::from_millis(150));
        let mut replacement = Peer::connect_profile(address, catalog, 0x780);
        notice(&mut replacement, "RegionEntered:demo:west");
        *released.0.lock().unwrap() = true;
        released.1.notify_all();
        sender.write(&ClientMessage::Chat {
            sequence: 2,
            text: "fresh".into(),
        });
        assert_eq!(chat(&mut sender).text, "[slow]");
        assert_eq!(
            chat(&mut replacement).text,
            "[fresh]",
            "old recipient's profile inherited pending chat from its retired session"
        );
    });
}

#[test]
fn chat_reload_contract_fences_module_entry_but_accepts_behavior_source_edits() {
    let fixture = fixture();
    let contract = || {
        fixture
            .startup(Arc::new(Catalog::builtins()))
            .unwrap()
            .reload_contract()
            .unwrap()
    };
    let original = contract();
    std::fs::write(
        fixture.0.join("packages/demo/chat.luau"),
        "return function(e) return {allow=true,text=e.text .. ' edited'} end",
    )
    .unwrap();
    assert!(
        original == contract(),
        "behavior source edits changed the frozen chat contract"
    );
    let entry = fixture.0.join("packages/demo/main.luau");
    let source = std::fs::read_to_string(&entry).unwrap().replace(
        "h.register_chat_hook('demo:chat',1,'demo:chat')",
        "h.register_chat_hook('demo:chat',1,'demo:players')",
    );
    std::fs::write(entry, source).unwrap();
    assert!(
        original != contract(),
        "chat module entry change retained old callback authority on reload"
    );
}
