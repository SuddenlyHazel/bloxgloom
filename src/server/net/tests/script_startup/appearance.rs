//! Exact startup negotiation, profile selection, peer replication and recovery
//! through the production nonblocking listener, never direct mutation helpers.
use super::*;
use crate::content::ContentManifest;
use crate::protocol::{PublicEntity, PublicEntityChange};
use crate::server::client_bundle::{CacheKey, ClientBundle};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[path = "appearance/characters.rs"]
mod characters;
#[path = "appearance/operations.rs"]
mod operations;
#[path = "appearance/player_models.rs"]
mod player_models;

const CALL: &str = "h.register_player_appearance('demo:wardrobe', 1, 'bloxgloom:humanoid/v1', {skins={{0.1,0.9,0.2}}, shirts={{0.9,0.1,0.2}}, pants={{0.1,0.2,0.9}}})";
fn source(extra: &str) -> String {
    format!("return function(h) {CALL}; {extra} end")
}

struct Peer {
    stream: TcpStream,
    catalog: Catalog,
    own: u64,
    views: BTreeMap<u64, PublicEntity>,
    epoch: u64,
    next_seq: u64,
    inventory_revision: u64,
}
impl Peer {
    fn connect(address: std::net::SocketAddr, profile: u128, expected: &Catalog) -> Self {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        protocol::write_client(
            &mut stream,
            &ClientMessage::Hello {
                name: "appearance".into(),
                profile,
                content_fingerprint: Catalog::builtins().fingerprint(),
            },
        )
        .unwrap();
        let ServerMessage::BundleOffer { identity } = protocol::read_server(&mut stream).unwrap()
        else {
            panic!("missing exact appearance bundle")
        };
        let bundle = crate::client::bundle::receive(&mut stream, identity, None).unwrap();
        assert!(
            bundle
                .bytes()
                .starts_with(if expected.models().count() == 0 {
                    b"BGCLIENT\x0d"
                } else {
                    b"BGCLIENT\x31"
                })
        );
        let local = bundle.session_catalog().unwrap();
        let (fingerprint, bytes) = receive_content_manifest(&mut stream);
        let catalog = ContentManifest::decode(&bytes)
            .unwrap()
            .resolve_catalog(&local)
            .unwrap();
        assert_eq!(catalog.fingerprint(), expected.fingerprint());
        assert_eq!(catalog.appearance_color(1, 8), Some([0.9, 0.1, 0.2]));
        protocol::write_client(&mut stream, &ClientMessage::ContentReady { fingerprint }).unwrap();
        let mut peer = Self {
            stream,
            catalog,
            own: 0,
            views: BTreeMap::new(),
            epoch: 0,
            next_seq: 1,
            inventory_revision: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if matches!(peer.read(deadline), ServerMessage::Inventory { .. }) {
                break;
            }
        }
        assert_ne!(peer.own, 0);
        peer.send(ClientMessage::SetView { radius: 1 });
        peer
    }
    fn send(&mut self, message: ClientMessage) {
        protocol::write_client_with_catalog(&mut self.stream, &message, &self.catalog).unwrap();
    }
    fn read(&mut self, deadline: Instant) -> ServerMessage {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("appearance response deadline");
        self.stream.set_read_timeout(Some(remaining)).unwrap();
        let message = protocol::read_server_with_catalog(&mut self.stream, &self.catalog).unwrap();
        match &message {
            ServerMessage::ActionSession {
                epoch, next_seq, ..
            } => {
                self.epoch = *epoch;
                self.next_seq = *next_seq;
            }
            ServerMessage::Inventory { revision, .. } => self.inventory_revision = *revision,
            ServerMessage::OwnedEntity { id } => self.own = *id,
            ServerMessage::EntitySnapshotPage(page) => {
                for view in &page.entities {
                    if view.entity_type == crate::content::EntityTypeId(2) {
                        self.views.insert(view.id, view.clone());
                    }
                }
            }
            ServerMessage::WorldCommitPart(part) => {
                for change in &part.entities {
                    match change {
                        PublicEntityChange::Upsert(view)
                            if view.entity_type == crate::content::EntityTypeId(2) =>
                        {
                            self.views.insert(view.id, view.clone());
                        }
                        PublicEntityChange::Remove { id, .. } => {
                            self.views.remove(id);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        assert!(self.views.len() <= 2);
        message
    }
    fn appearance(&mut self, id: u64, expected: [u8; 4]) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self
            .views
            .get(&id)
            .is_none_or(|view| view.payload != expected)
        {
            self.read(deadline);
        }
        self.views[&id].revision
    }
    fn barrier(&mut self) {
        self.send(ClientMessage::Ping { nonce: 0xface });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !matches!(self.read(deadline), ServerMessage::Pong { nonce: 0xface }) {}
    }
}

#[test]
fn registered_appearance_selection_replicates_and_restarts_by_profile() {
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, &source(""));
    for restarted in [false, true] {
        let state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        gameplay::serve(state, |address| {
            let mut first = Peer::connect(address, 0xa991, &catalog);
            let mut second = Peer::connect(address, 0xa992, &catalog);
            let a = first.own;
            let b = second.own;
            let expected = if restarted { [6, 8, 6, 0] } else { [0; 4] };
            first.appearance(a, expected);
            second.appearance(a, expected);
            first.appearance(b, [0; 4]);
            second.appearance(b, [0; 4]);
            if !restarted {
                first.send(ClientMessage::SelectAppearance {
                    palettes: [6, 8, 6],
                });
                let revision = first.appearance(a, [6, 8, 6, 0]);
                assert_eq!(revision, 2);
                assert_eq!(second.appearance(a, [6, 8, 6, 0]), revision);
                // Retry is an idempotent replacement, not another entity mutation.
                first.send(ClientMessage::SelectAppearance {
                    palettes: [6, 8, 6],
                });
                first.barrier();
                assert_eq!(first.views[&a].revision, revision);
                assert_eq!(first.views[&b].payload, [0; 4]);
                let saved = fixture
                    .0
                    .join("save/players/0000000000000000000000000000a991.appearance");
                let bytes = std::fs::read(&saved).unwrap();
                // A raw byte cannot select an undeclared color. Rejection closes
                // only its session and leaves the last committed profile intact.
                first.send(ClientMessage::SelectAppearance {
                    palettes: [255, 8, 6],
                });
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let remaining = deadline
                        .checked_duration_since(Instant::now())
                        .expect("invalid selection was not rejected");
                    first.stream.set_read_timeout(Some(remaining)).unwrap();
                    if let Err(error) =
                        protocol::read_server_with_catalog(&mut first.stream, &first.catalog)
                    {
                        assert!(
                            !matches!(
                                error.kind(),
                                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                            ),
                            "invalid selection did not close the session"
                        );
                        break;
                    }
                }
                assert_eq!(std::fs::read(saved).unwrap(), bytes);
                second.barrier();
            } else {
                assert_eq!(
                    first.views[&a].revision, 1,
                    "session revision restarts, profile does not"
                );
            }
        });
    }
}

#[test]
fn appearance_bundle_composes_with_rules_animation_and_exact_world_identity() {
    let fixture = Fixture::new();
    for extra in [
        String::new(),
        format!("h.register_player_rules('demo:small', 1, {})", player::FIELDS),
        "h.register_item('demo:token','Token','bloxgloom:stone',{drop_size='small',drop_animation={pop_height=1.25}})".into(),
        format!("h.register_player_rules('demo:small', 1, {}); h.register_item('demo:token','Token','bloxgloom:stone',{{drop_animation={{pop_height=1.25}}}})", player::FIELDS),
        "h.register_item('demo:token','Token','bloxgloom:stone',{drop_policy={lifetime_ms=2000}})".into(),
        format!("h.register_player_rules('demo:small', 1, {}); h.register_item('demo:token','Token','bloxgloom:stone',{{drop_size='small',drop_animation={{pop_height=1.25}},drop_policy={{lifetime_ms=2000}}}})", player::FIELDS),
    ] {
        fixture.package("demo", CONTENT, &source(&extra));
        let startup = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
        let bundle = startup.client_bundle.as_ref().unwrap();
        assert!(bundle.bytes().starts_with(if extra.contains("drop_policy") {
            b"BGCLIENT\x16"
        } else {
            b"BGCLIENT\x0d"
        }));
        let manifest = ContentManifest::from_catalog(&startup.catalog());
        let decoded = ClientBundle::decode_verify(bundle.bytes(), bundle.cache_key()).unwrap();
        assert_eq!(decoded.session_catalog().unwrap().fingerprint(), startup.catalog().fingerprint());
        let mut changed = bundle.bytes().to_vec();
        let offset = changed.len() - 4;
        changed[offset..].copy_from_slice(&0.5f32.to_le_bytes());
        assert!(ClientBundle::decode_verify(&changed, bundle.cache_key()).is_err());
        let key = CacheKey::from_bytes(Sha256::digest(&changed).into());
        let different = ClientBundle::decode_verify(&changed, key).unwrap().session_catalog().unwrap();
        assert!(manifest.resolve_catalog(&different).is_err());
        for version in 7..=12 {
            let mut bytes = bundle.bytes().to_vec();
            bytes[8] = version;
            let key = CacheKey::from_bytes(Sha256::digest(&bytes).into());
            assert!(ClientBundle::decode_verify(&bytes, key).is_err());
        }
    }
    fixture.package("demo", CONTENT, &source(""));
    drop(fixture.open().unwrap());
    let path = fixture.0.join("save/content.map");
    let original = std::fs::read(&path).unwrap();
    for changed in [
        source("").replace("0.1,0.9,0.2", "0.2,0.9,0.2"),
        source("").replace("wardrobe", "other"),
        "return function(_) end".into(),
    ] {
        fixture.package("demo", CONTENT, &changed);
        assert!(fixture.open().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn invalid_or_caught_appearance_declarations_never_publish() {
    for call in [
        CALL.replace("demo:wardrobe", "other:wardrobe"),
        CALL.replace("humanoid/v1", "unknown/v1"),
        CALL.replace("0.1,0.9,0.2", "0/0,0.9,0.2"),
        CALL.replace("0.1,0.9,0.2", "2,0.9,0.2"),
        CALL.replace("{{0.1,0.9,0.2}}", "{[2]={0.1,0.9,0.2}}"),
        CALL.replace("{{0.1,0.9,0.2}}", "table.create(25, {0.1,0.9,0.2})"),
        format!("{CALL}; {CALL}"),
    ] {
        let fixture = Fixture::new();
        fixture.package(
            "demo",
            CONTENT,
            &format!("return function(h) pcall(function() {call} end) end"),
        );
        assert!(fixture.open().is_err(), "{call}");
        assert!(!fixture.0.join("save").exists());
    }
    let denied = Fixture::new();
    denied.package("demo", "", &source(""));
    assert!(denied.open().is_err());
    assert!(!denied.0.join("save").exists());
    let duplicate = Fixture::new();
    duplicate.package("demo", CONTENT, &source(""));
    duplicate.package(
        "other",
        CONTENT,
        &source("").replace("demo:wardrobe", "other:wardrobe"),
    );
    assert!(duplicate.open().is_err());
    assert!(!duplicate.0.join("save").exists());
}
