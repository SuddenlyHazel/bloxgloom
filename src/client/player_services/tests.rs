use super::*;

use std::{path::PathBuf, time::Duration};

struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-client-player-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        for owner in ["demo", "other"] {
            let directory = path.join(owner);
            std::fs::create_dir_all(directory.join("server")).unwrap();
            std::fs::create_dir_all(directory.join("client")).unwrap();
            std::fs::write(directory.join("package.txt"),format!("format 2\npackage {owner}\nversion 1.0.0\nentry main\nrequires bloxgloom:players/v1\nmodule server main server/main.luau\nmodule client client_startup client/startup.luau\nmodule client player client/player.luau\n")).unwrap();
            std::fs::write(directory.join("server/main.luau"), "return function() end").unwrap();
            std::fs::write(
                directory.join("client/startup.luau"),
                format!("return function(h) h.set_player_handler('{owner}:player') end"),
            )
            .unwrap();
            std::fs::write(directory.join("client/player.luau"), if owner=="demo" {source} else {"return function(_,e) assert(e.states['demo:progress']==nil and e.states['other:progress'].public=='other') end"}).unwrap();
        }
        Self(path)
    }
    fn bundle(&self) -> Arc<ClientBundle> {
        crate::server::package_bundle_for_preview(&self.0).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn states(value: &str) -> Vec<PlayerState> {
    vec![
        PlayerState {
            key: "demo:progress".into(),
            revision: u64::MAX,
            public: value.as_bytes().to_vec(),
        },
        PlayerState {
            key: "other:progress".into(),
            revision: 1,
            public: b"other".to_vec(),
        },
    ]
}

#[test]
fn client_player_callbacks_compose_filter_public_state_and_disconnect_without_queue_room() {
    let fixture = Fixture::new(
        r#"return function(h,e)
        assert(tostring(e.profile)=='profile:ffffffffffffffffffffffffffffffff')
        assert(tostring(e.session)=='session:ffffffffffffffffffffffffffffffff:ffffffffffffffff')
        assert(e.states['other:progress']==nil and e.state==nil)
        assert(not pcall(function() e.states['demo:progress'].public='private' end))
        assert(not pcall(function() e.profile=1 end))
        if e.kind=='SessionReady' then assert(e.states['demo:progress'].public=='ready')
        elseif e.kind=='PlayerStateChanged' then assert(e.states['demo:progress'].public=='changed' or e.states['demo:progress'].public=='final')
        elseif e.kind=='SessionDisconnected' then assert(e.reason=='closed' and e.states['demo:progress'].public=='final')
        else error('unexpected event') end
    end"#,
    );
    let bundle = fixture.bundle();
    let startup = startup::prepare(Arc::clone(&bundle)).unwrap();
    assert_eq!(startup.player_handlers.len(), 2);
    let mut lane = Lane::spawn(bundle, &startup, u128::MAX, u64::MAX, states("ready"))
        .unwrap()
        .unwrap();
    for _ in 0..2 {
        lane.replies.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert_eq!(
        lane.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        ("SessionReady", 0)
    );
    lane.update(u128::MAX, u64::MAX, states("changed"));
    for _ in 0..2 {
        lane.replies.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert_eq!(
        lane.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        ("PlayerStateChanged", 0)
    );
    // Leave an update reply unread so the worker can be backpressured. The close
    // signal bypasses this slot and dropping the receiver wakes its blocked send.
    lane.update(u128::MAX, u64::MAX, states("changed"));
    lane.update(u128::MAX, u64::MAX, states("final"));
    lane.close("closed");
    let (_, replacement) = mpsc::sync_channel(1);
    drop(std::mem::replace(&mut lane.replies, replacement));
    let (_, replacement) = mpsc::channel();
    let events = std::mem::replace(&mut lane.events, replacement);
    drop(lane);
    let mut disconnected = false;
    while let Ok((kind, failures)) = events.recv_timeout(Duration::from_secs(5)) {
        if kind == "SessionDisconnected" {
            assert_eq!(failures, 0);
            disconnected = true;
            break;
        }
    }
    assert!(disconnected);
}

#[test]
fn client_player_callback_rejection_is_atomic_and_other_packages_keep_running() {
    let fixture = Fixture::new(
        r#"return function(h,e)
        if e.kind=='SessionReady' then
            pcall(function() h.set_state('other:document','bad') end)
        end
    end"#,
    );
    let bundle = fixture.bundle();
    let startup = startup::prepare(Arc::clone(&bundle)).unwrap();
    let lane = Lane::spawn(bundle, &startup, 1, 1, states("ready"))
        .unwrap()
        .unwrap();
    let output = lane.replies.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(output.texts.is_empty() && output.states.is_empty());
    assert_eq!(
        lane.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        ("SessionReady", 1)
    );
    lane.update(1, 1, states("changed"));
    for _ in 0..2 {
        lane.replies.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    assert_eq!(
        lane.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        ("PlayerStateChanged", 0)
    );
    lane.close("closed");
    assert_eq!(
        lane.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        ("SessionDisconnected", 0)
    );
}

#[test]
fn retained_player_modules_keep_imports_and_coroutines_but_revoke_old_hosts() {
    let fixture = Fixture::new(
        r#"
        local cache = import('demo:cache')
        local calls = 0
        local saved_host
        local task = coroutine.create(function(host, event)
            assert(event.kind == 'SessionReady')
            coroutine.yield()
            assert(not pcall(function() host.set_parameter('bad', 'bad', 1) end))
            -- Copied readonly input remains historical data, not host authority.
            assert(event.kind == 'SessionReady')
        end)
        return function(host, event)
            calls += 1
            cache.calls += 1
            assert(calls == cache.calls)
            if calls == 1 then
                assert(event.kind == 'SessionReady')
                saved_host = host
                assert(coroutine.resume(task, host, event))
            else
                assert(calls == 2 and event.kind == 'PlayerStateChanged')
                assert(not pcall(function() saved_host.set_state('demo:doc', 'stale') end))
                assert(coroutine.resume(task))
                assert(coroutine.status(task) == 'dead')
            end
        end
    "#,
    );
    let manifest = fixture.0.join("demo/package.txt");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("module client cache client/cache.luau\n");
    std::fs::write(manifest, text).unwrap();
    std::fs::write(
        fixture.0.join("demo/client/cache.luau"),
        "return { calls = 0 }",
    )
    .unwrap();
    let bundle = fixture.bundle();
    let mut realm = startup::EventRealm::new(&bundle, "demo:player").unwrap();
    let public = states("ready");
    for kind in ["SessionReady", "PlayerStateChanged"] {
        let mut output = State::default();
        let event = Event {
            kind,
            profile: 1,
            session: 2,
            states: &public,
            reason: "",
        };
        startup::execute_retained(
            &mut realm,
            Arc::clone(&bundle),
            "demo:player",
            &mut output,
            Some(&event),
        )
        .unwrap();
        assert!(output.states.is_empty());
    }
    // A reconnect receives a fresh entry and dependency export, never the
    // prior connection's counter or suspended coroutine.
    let mut reconnected = startup::EventRealm::new(&bundle, "demo:player").unwrap();
    let event = Event {
        kind: "SessionReady",
        profile: 1,
        session: 3,
        states: &public,
        reason: "",
    };
    startup::execute_retained(
        &mut reconnected,
        bundle,
        "demo:player",
        &mut State::default(),
        Some(&event),
    )
    .unwrap();
}

#[test]
fn failed_client_import_initialization_is_cached_for_the_realm() {
    let fixture = Fixture::new(
        r#"
        return function()
            local first_ok, first = pcall(import, 'demo:broken')
            math.random()
            local second_ok, second = pcall(import, 'demo:broken')
            assert(not first_ok and not second_ok)
            -- Rust callback error envelopes have their own call-site traceback;
            -- the failing initializer's random payload must remain identical.
            local payload = 'broken"%]:1: ([%d%.e%+%-]+)'
            local first_draw = string.match(tostring(first), payload)
            assert(first_draw and first_draw == string.match(tostring(second), payload))
        end
    "#,
    );
    let manifest = fixture.0.join("demo/package.txt");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("module client broken client/broken.luau\n");
    std::fs::write(manifest, text).unwrap();
    std::fs::write(
        fixture.0.join("demo/client/broken.luau"),
        "error(tostring(math.random()))",
    )
    .unwrap();
    let bundle = fixture.bundle();
    let mut realm = startup::EventRealm::new(&bundle, "demo:player").unwrap();
    let event = Event {
        kind: "SessionReady",
        profile: 1,
        session: 2,
        states: &[],
        reason: "",
    };
    startup::execute_retained(
        &mut realm,
        bundle,
        "demo:player",
        &mut State::default(),
        Some(&event),
    )
    .unwrap();
}
