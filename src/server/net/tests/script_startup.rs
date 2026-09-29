//! Package discovery -> worker -> public registrar -> world -> real listener.
use super::*;
use crate::content::Catalog;
use crate::inventory::Inventory;
use crate::server::startup::ServerStartup;
use std::path::PathBuf;
use std::sync::Arc;

#[path = "script_startup/appearance.rs"]
mod appearance;
#[path = "script_startup/bundle.rs"]
mod bundle;
#[path = "script_startup/bundle_catalog.rs"]
mod bundle_catalog;
#[path = "script_startup/bundle_runtime.rs"]
mod bundle_runtime;
#[path = "script_startup/bundle_ui.rs"]
mod bundle_ui;
#[path = "script_startup/combined.rs"]
mod combined;
#[path = "script_startup/drop_policy.rs"]
mod drop_policy;
#[path = "script_startup/gameplay.rs"]
mod gameplay;
#[path = "script_startup/generation.rs"]
mod generation;
#[path = "script_startup/join_lifecycle.rs"]
mod join_lifecycle;
#[path = "script_startup/player.rs"]
mod player;
#[path = "script_startup/system.rs"]
mod system;
#[path = "script_startup/tags.rs"]
mod tags;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bloxgloom-script-startup-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        std::fs::create_dir(path.join("packages")).unwrap();
        Self(path)
    }

    fn package(&self, name: &str, requires: &str, source: &str) {
        let dir = self.0.join("packages").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.txt"), format!(
            "format 1\npackage {name}\nversion 1.0.0\nentry main\nmodule main main.luau\n{requires}\n"
        )).unwrap();
        std::fs::write(dir.join("main.luau"), source).unwrap();
    }

    fn startup(&self, catalog: Arc<Catalog>) -> io::Result<ServerStartup> {
        ServerStartup::new(catalog).with_local_packages(&self.0.join("packages"))
    }

    fn open(&self) -> io::Result<State> {
        crate::server::server_state_with_startup(
            7,
            self.0.join("save"),
            2,
            self.startup(Arc::new(Catalog::builtins()))?,
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

const CONTENT: &str = "requires bloxgloom:content/v1";
const TOKEN: &str =
    "return function(host) host.register_item('demo:token', 'Token', 'bloxgloom:stone') end";

#[test]
fn luau_startup_item_reaches_listener_inventory_and_restart() {
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, TOKEN);
    let builtin = Catalog::builtins();
    let mut expected = None;
    for restarted in [false, true] {
        // Declaration order is not identity order, including across restart.
        let entries = if restarted {
            "h.register_item('demo:token', 'Token', 'bloxgloom:stone'); h.register_item('demo:other', 'Other', 'bloxgloom:stone')"
        } else {
            "h.register_item('demo:other', 'Other', 'bloxgloom:stone'); h.register_item('demo:token', 'Token', 'bloxgloom:stone')"
        };
        fixture.package(
            "demo",
            CONTENT,
            &format!("return function(h) {entries} end"),
        );
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        let item = catalog
            .items()
            .find(|item| item.key == "demo:token")
            .unwrap();
        let id = item.id;
        assert!(item.placeable.is_none());
        for item in builtin.items() {
            assert_eq!(catalog.item(item.id).unwrap().key, item.key);
        }
        for state in crate::content::ContentManifest::from_catalog(&builtin).entries {
            if state.kind == b'S' {
                assert_eq!(
                    catalog.state_by_key(&state.key),
                    Some(crate::content::BlockStateId(state.id))
                );
            }
        }
        let manifest = crate::content::ContentManifest::from_catalog(&catalog)
            .encode()
            .unwrap();
        let identity = (id, catalog.fingerprint(), manifest.clone());
        if restarted {
            assert_eq!(expected.as_ref(), Some(&identity));
        } else {
            expected = Some(identity);
            let mut inventory = Inventory::default();
            assert_eq!(inventory.insert_with_catalog(id, 128, &catalog), 0);
            state.inventory_store.save(0x5152, &inventory).unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (stop_tx, stop_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || reactor::serve_listener_until(listener, state, stop_rx));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "luau-startup".into(),
                    profile: 0x5152,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, received) = receive_content_manifest(&mut peer);
            assert_eq!(fingerprint, catalog.fingerprint());
            assert_eq!(received, manifest);
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                assert!(Instant::now() < deadline, "startup inventory not received");
                if let ServerMessage::Inventory { slots, .. } =
                    protocol::read_server_with_catalog(&mut peer, &catalog).unwrap()
                {
                    assert!(
                        slots
                            .iter()
                            .flatten()
                            .any(|stack| stack.item == id && stack.count == 128)
                    );
                    break;
                }
            }
        }));
        stop_tx.send(()).unwrap();
        server.join().unwrap().unwrap();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}

#[test]
fn luau_startup_rejections_publish_nothing_and_never_open_world() {
    let cases = [
        (
            CONTENT,
            "return function(h) h.register_item('bad:first', 'First', 'bloxgloom:stone'); error('abort startup') end",
            "abort startup",
        ),
        (
            CONTENT,
            "return function(h) h.register_item('demo:stolen', 'Stolen', 'bloxgloom:stone') end",
            "namespace",
        ),
        (
            "",
            "return function(h) h.register_item('bad:token', 'Token', 'bloxgloom:stone') end",
            "requires",
        ),
        (
            "requires bloxgloom:machines/v1",
            "return function(_) end",
            "unsupported",
        ),
        (
            CONTENT,
            "return function(h) h.register_item('bad:token', 'Token', 'bloxgloom:missing') end",
            "missing texture",
        ),
        (
            CONTENT,
            "return function(h) h.register_item('bad:token', 'Token', 'bloxgloom:stone'); pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone') end) end",
            "duplicate",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item({}, 'Token', 'bloxgloom:stone') end) end",
            "three UTF-8",
        ),
        (
            CONTENT,
            "return function(h) h.register_item('bad:token', string.rep('x', 256), 'bloxgloom:stone') end",
            "255 bytes",
        ),
        (
            CONTENT,
            "return function(h) for i = 1, 33 do pcall(function() h.register_item('bad:item' .. i, 'Item', 'bloxgloom:stone') end) end end",
            "limit exceeded",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { sprite = 'false' }) end) end",
            "sprite option must be boolean",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { unknown = true }) end) end",
            "unknown item option",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', false) end) end",
            "item options must be a table",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { drop_size = 'huge' }) end) end",
            "drop_size option",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { drop_size = 1 }) end) end",
            "drop_size option",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { drop_animation = { pop_duration = -1 } }) end) end",
            "invalid drop_animation range",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { drop_animation = { pickup_arc = 0/0 } }) end) end",
            "invalid drop_animation range",
        ),
        (
            CONTENT,
            "return function(h) pcall(function() h.register_item('bad:token', 'Token', 'bloxgloom:stone', { drop_animation = { wobble = 1 } }) end) end",
            "unknown drop_animation field",
        ),
    ];
    for (requires, source, message) in cases {
        let fixture = Fixture::new();
        fixture.package("demo", CONTENT, TOKEN);
        fixture.package("bad", requires, source);
        let base = Arc::new(Catalog::builtins());
        let before = base.fingerprint();
        let error = fixture.startup(Arc::clone(&base)).err().unwrap();
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(base.fingerprint(), before);
        assert!(base.items().all(|item| item.key != "demo:token"));
        assert!(fixture.open().is_err());
        assert!(!fixture.0.join("save").exists());
    }
}

#[test]
fn luau_failed_restart_leaves_existing_save_unchanged_and_can_retry() {
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, TOKEN);
    drop(fixture.open().unwrap());
    let files = ["content.map", "server.wal"];
    let before = files.map(|file| std::fs::read(fixture.0.join("save").join(file)).unwrap());
    for source in [
        "return function(_) error('late failure') end",
        "return function(h) h.register_item('zfailure:token', 'Token', 'bloxgloom:missing') end",
    ] {
        fixture.package("zfailure", CONTENT, source);
        assert!(fixture.open().is_err());
        for (file, bytes) in files.iter().zip(&before) {
            assert_eq!(
                &std::fs::read(fixture.0.join("save").join(file)).unwrap(),
                bytes
            );
        }
    }
    std::fs::remove_dir_all(fixture.0.join("packages/zfailure")).unwrap();
    let recovered = fixture.open().unwrap();
    assert!(
        recovered
            .world
            .catalog()
            .items()
            .any(|item| item.key == "demo:token")
    );
}

#[test]
fn luau_startup_validates_contracts_and_runs_imports_at_the_item_bound() {
    let fixture = Fixture::new();
    fixture.package("helper", "", "return function(_) end");
    std::fs::write(fixture.0.join("packages/helper/package.txt"),
        "format 1\npackage helper\nversion 1.2.3\nentry main\nmodule main main.luau\nmodule naming naming.luau\n"
    ).unwrap();
    std::fs::write(
        fixture.0.join("packages/helper/naming.luau"),
        "return function(i) return 'demo:item' .. i end",
    )
    .unwrap();
    fixture.package("demo", &format!("{CONTENT}\ndependency helper 1.2.3"),
        "local name = import('helper:naming'); return function(h) for i = 1, 32 do h.register_item(name(i), 'Item', 'bloxgloom:stone') end end");
    let startup = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    assert_eq!(
        startup
            .catalog()
            .items()
            .filter(|item| item.key.starts_with("demo:"))
            .count(),
        32
    );
    fixture.package("bloxgloom", CONTENT, "return function(_) end");
    assert!(
        fixture
            .open()
            .err()
            .unwrap()
            .to_string()
            .contains("reserved namespace")
    );
    assert!(!fixture.0.join("save").exists());
}
