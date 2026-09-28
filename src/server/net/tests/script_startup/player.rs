//! Startup -> verified artifact -> live client negotiation and persisted identity.
use super::*;
use crate::content::ContentManifest;
use crate::server::client_bundle::{CacheKey, ClientBundle};
use sha2::{Digest, Sha256};

pub(super) const FIELDS: &str = "{half_width=0.2, foot_inset=0.025, middle_height=0.4, head_height=0.75, intent_blocks_per_second=2, budget_blocks_per_second=3, headroom=4, max_rise=8, eye_height=0.65}";

pub(super) fn source(key: &str, revision: u32, fields: &str) -> String {
    format!("return function(h) h.register_player_rules('{key}', {revision}, {fields}) end")
}

#[test]
fn custom_player_rules_negotiate_before_welcome_and_survive_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, &source("demo:small", 1, FIELDS));
    let mut identity = None;
    for _ in 0..2 {
        let state = Box::new(fixture.open().unwrap());
        let server = state.world.catalog_arc();
        let expected = ContentManifest::from_catalog(&server);
        if let Some(previous) = &identity {
            assert_eq!(previous, &expected);
        }
        identity = Some(expected.clone());
        let rules = server.player_rules();
        assert_eq!(rules.body().head_height, 0.75);
        assert_eq!(rules.body().half_width, 0.2);
        assert_eq!(rules.motion().intent_blocks_per_second, 2.0);
        assert_eq!(rules.motion().budget_blocks_per_second, 3.0);
        assert_eq!(rules.spawn().headroom, 4);
        assert_eq!(rules.spawn().max_rise, 8);
        assert_eq!(rules.eye_height(), 0.65);
        assert!(
            state
                .client_bundle
                .as_ref()
                .unwrap()
                .bytes()
                .starts_with(b"BGCLIENT\x0a")
        );
        gameplay::serve(state, |address| {
            let client = crate::client::connect_catalog_probe(&address.to_string(), 0x901).unwrap();
            assert_eq!(client.player_rules(), rules);
            assert_eq!(client.fingerprint(), server.fingerprint());
            assert_eq!(ContentManifest::from_catalog(&client), expected);
            check_movement(client, Arc::clone(&server));
        });
    }
    let path = fixture.0.join("save/content.map");
    let original = std::fs::read(&path).unwrap();
    for changed in [
        source("demo:small", 2, FIELDS),
        source("demo:other", 1, FIELDS),
        source(
            "demo:small",
            1,
            &FIELDS.replace("eye_height=0.65", "eye_height=0.6"),
        ),
        "return function(_) end".into(),
    ] {
        fixture.package("demo", CONTENT, &changed);
        assert!(
            fixture.open().is_err(),
            "changed rules must refuse saved world"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    fixture.package("demo", CONTENT, &source("demo:small", 1, FIELDS));
    assert!(
        fixture.open().is_ok(),
        "restoring exact selection remains retryable"
    );
}

fn check_movement(client: Arc<Catalog>, server: Arc<Catalog>) {
    use crate::server::movement::{
        MovementCommand, MovementState, StopReason, process_movement_batch,
    };
    use crate::server::voxel_view::VoxelView;
    use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, STONE};
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    blocks[Chunk::index([1, 2, 1]).unwrap()] = STONE;
    let chunk = Chunk::from_blocks(ChunkKey { x: 0, y: 0, z: 0 }, 1, blocks);
    let view = VoxelView::from_chunks_in([chunk], server).unwrap();
    let feet = [1.5, 1.0, 1.5];
    let delta = [
        client.player_rules().motion().intent_blocks_per_second * 0.02,
        0.0,
        0.0,
    ];
    // The shared production prediction solver clears a ceiling that blocks the
    // builtin tall body; authority uses the negotiated body and budget too.
    let predicted = crate::physics::resolve_player_movement(
        client.player_rules().body(),
        feet,
        delta,
        |x, y, z| Ok::<_, ()>([x, y, z] == [1, 2, 1]),
    )
    .unwrap();
    assert!(predicted[0] > feet[0]);
    let moved = process_movement_batch(
        &view,
        MovementState::new(feet, 0),
        &[MovementCommand { seq: 1, delta }],
    );
    assert_eq!(moved.consumed, 1);
    assert_eq!(moved.state.position(), predicted);
    let blocked = process_movement_batch(
        &view,
        MovementState::new(feet, 0),
        &[MovementCommand {
            seq: 1,
            delta: [0.1, 0.0, 0.0],
        }],
    );
    assert_eq!(blocked.stop_reason, StopReason::MovementBudget);
    assert_eq!(blocked.consumed, 0);
}

#[test]
fn rejected_player_declarations_including_caught_errors_never_open_world() {
    for (requires, call) in [
        (
            "",
            format!("h.register_player_rules('demo:small', 1, {FIELDS})"),
        ),
        (
            CONTENT,
            format!("h.register_player_rules('other:small', 1, {FIELDS})"),
        ),
        (
            CONTENT,
            format!("h.register_player_rules('demo:small', 0, {FIELDS})"),
        ),
        (
            CONTENT,
            format!("h.register_player_rules('demo:small', 1.5, {FIELDS})"),
        ),
        (CONTENT, "h.register_player_rules({}, 1, {})".into()),
        (
            CONTENT,
            "h.register_player_rules('demo:small', 1, {})".into(),
        ),
        (
            CONTENT,
            format!(
                "h.register_player_rules('demo:small', 1, {} )",
                FIELDS.replace("headroom=4", "headroom=4.5")
            ),
        ),
        (
            CONTENT,
            format!(
                "h.register_player_rules('demo:small', 1, {} )",
                FIELDS.replace("half_width=0.2", "half_width=0/0")
            ),
        ),
        (
            CONTENT,
            format!(
                "h.register_player_rules('demo:small', 1, {} )",
                FIELDS.replace("eye_height=0.65", "eye_height=2")
            ),
        ),
        (
            CONTENT,
            format!(
                "h.register_player_rules('demo:small', 1, {} )",
                FIELDS.replace("headroom=4", "unknown=4")
            ),
        ),
        (
            CONTENT,
            format!(
                "h.register_player_rules('demo:small', 1, {FIELDS}); h.register_player_rules('demo:small', 1, {FIELDS})"
            ),
        ),
    ] {
        let fixture = Fixture::new();
        fixture.package(
            "demo",
            requires,
            &format!("return function(h) pcall(function() {call} end) end"),
        );
        assert!(fixture.open().is_err(), "{call}");
        assert!(!fixture.0.join("save").exists());
    }
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, &source("demo:small", 1, FIELDS));
    fixture.package("other", CONTENT, &source("other:small", 1, FIELDS));
    assert!(fixture.open().is_err());
    assert!(!fixture.0.join("save").exists());
}

#[test]
fn player_artifact_tampering_is_rejected_or_fails_exact_manifest_match() {
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, &source("demo:small", 1, FIELDS));
    let startup = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let bundle = startup.client_bundle.as_ref().unwrap();
    let manifest = ContentManifest::from_catalog(&startup.catalog());
    let bytes = bundle.bytes();
    // The fixed-size rule record is the final 40 bytes. Invalid geometry fails
    // decode even if the attacker also recomputes the artifact digest.
    for offset in [0, 4, 8, 12, 16, 20, 24, 32, 36] {
        let mut tampered = bytes.to_vec();
        let start = bytes.len() - 40 + offset;
        tampered[start..start + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ClientBundle::decode_verify(&tampered, bundle.cache_key()).is_err());
        let key = CacheKey::from_bytes(Sha256::digest(&tampered).into());
        if let Ok(decoded) = ClientBundle::decode_verify(&tampered, key) {
            assert!(
                manifest
                    .resolve_catalog(&decoded.session_catalog().unwrap())
                    .is_err()
            );
        }
    }
    // Valid but different rules cannot join either, even with a valid SHA.
    let mut tampered = bytes.to_vec();
    let start = bytes.len() - 40 + 20;
    tampered[start..start + 4].copy_from_slice(&0.6f32.to_le_bytes());
    let key = CacheKey::from_bytes(Sha256::digest(&tampered).into());
    let different = ClientBundle::decode_verify(&tampered, key)
        .unwrap()
        .session_catalog()
        .unwrap();
    assert!(manifest.resolve_catalog(&different).is_err());
    assert_ne!(different.fingerprint(), startup.catalog().fingerprint());
    for truncated in [&bytes[..bytes.len() - 1], &bytes[..bytes.len() - 44]] {
        let key = CacheKey::from_bytes(Sha256::digest(truncated).into());
        assert!(ClientBundle::decode_verify(truncated, key).is_err());
    }
    for mode in 0..4 {
        let mut malformed = bytes.to_vec();
        let key_start = bytes
            .windows(b"demo:small".len())
            .position(|w| w == b"demo:small")
            .unwrap();
        match mode {
            0 => malformed[key_start..key_start + 4].copy_from_slice(b"evil"),
            1 => malformed[bytes.len() - 48..bytes.len() - 44].fill(0),
            2 => malformed.extend_from_slice(&bytes[key_start - 4..]),
            3 => malformed[8] = 7, // Old format cannot silently discard custom data.
            _ => unreachable!(),
        }
        let key = CacheKey::from_bytes(Sha256::digest(&malformed).into());
        assert!(ClientBundle::decode_verify(&malformed, key).is_err());
    }
}

#[test]
fn player_rules_compose_with_existing_sized_item_artifacts() {
    let fixture = Fixture::new();
    let declaration = source("demo:small", 1, FIELDS).replace(
        " end",
        "; h.register_item('demo:token', 'Token', 'bloxgloom:stone', {drop_size='small'}) end",
    );
    fixture.package("demo", CONTENT, &declaration);
    let startup = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let bundle = startup.client_bundle.as_ref().unwrap();
    assert!(bundle.bytes().starts_with(b"BGCLIENT\x0b"));
    let verified = ClientBundle::decode_verify(bundle.bytes(), bundle.cache_key()).unwrap();
    let catalog = verified.session_catalog().unwrap();
    assert_eq!(catalog.player_rules(), startup.catalog().player_rules());
    assert_eq!(catalog.fingerprint(), startup.catalog().fingerprint());
}
