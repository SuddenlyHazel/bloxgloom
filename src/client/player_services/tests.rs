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
