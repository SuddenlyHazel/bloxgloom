//! Startup-frozen lifecycle identity, real listener publication and WAL restart.
use super::*;
use crate::server::{drops, durable, entities};
use bloxgloom_host_api::content::DropPolicy;

fn commit(state: &mut State, batch: entities::PreparedEntityBatch) {
    let permit = state
        .durability
        .entity_mirror
        .try_reserve_durable()
        .unwrap()
        .unwrap();
    let action = durable::CommitAction {
        client_id: None,
        profile: None,
        action_id: None,
        receipt_value: None,
        receipt_transition: None,
        inventory_before: None,
        inventory: None,
        world_edits: Vec::new(),
        terrain_reads: Default::default(),
        deltas: Vec::new(),
        changed_cells: Vec::new(),
        pickups: Vec::new(),
        fire_seed: None,
        entities: Some(batch),
        entity_wakes: Vec::new(),
    };
    assert!(
        state
            .durability
            .try_stage(
                crate::server::simulation::TickId::new(1),
                &action,
                Some(permit)
            )
            .unwrap()
    );
    durable::complete_barrier(state, durable::CommitBarrier::AllStaged).unwrap();
    assert!(state.durability.pending.is_empty());
}

#[test]
fn custom_drop_policy_negotiates_live_expiry_and_preserves_birth_across_restart() {
    let _cache = crate::client::bundle::TEST_CACHE_LOCK.lock().unwrap();
    let fixture = Fixture::new();
    let source = "return function(h)
        h.register_item('demo:long', 'Long', 'bloxgloom:stone', {drop_policy={gravity=0, terminal_speed=4, radius=0.4, pickup_range=5, merge_range=3, lifetime_ms=3600000}})
        h.register_item('demo:short', 'Short', 'bloxgloom:stone', {drop_policy={gravity=0, lifetime_ms=1000}})
        end";
    fixture.package("demo", CONTENT, source);
    let now = drops::unix_ms();
    let mut long_id = None;
    let mut short_id = None;
    for restarted in [false, true] {
        let mut state = Box::new(fixture.open().unwrap());
        let catalog = state.world.catalog_arc();
        let long = catalog.item_by_key("demo:long").unwrap();
        let short = catalog.item_by_key("demo:short").unwrap();
        let position = [
            state.spawn_anchor[0] + 8.0,
            state.spawn_anchor[1] + 4.0,
            state.spawn_anchor[2],
        ];
        assert_eq!(catalog.drop_policy(long).lifetime_ms, 3_600_000);
        assert!(
            state
                .client_bundle
                .as_ref()
                .unwrap()
                .bytes()
                .starts_with(b"BGCLIENT\x14")
        );
        if !restarted {
            // Older than the stock lifetime, but still live under this item's policy.
            let batch = drops::plan_spawns(
                &state.entities,
                &catalog,
                &[(position, long, 129, Duration::ZERO)],
                1,
                now - 700_000,
            )
            .unwrap()
            .unwrap();
            long_id = Some(batch.entity_ids()[0]);
            commit(&mut state, batch);
            // Short policy must expire this drop even though stock would keep it.
            let batch = drops::plan_spawns(
                &state.entities,
                &catalog,
                &[(position, short, 3, Duration::ZERO)],
                1,
                now - 10_000,
            )
            .unwrap()
            .unwrap();
            short_id = Some(batch.entity_ids()[0]);
            commit(&mut state, batch);
            assert!(drops::has_expired(&state.entities, now));
        } else {
            assert!(state.entities.snapshot(short_id.unwrap()).is_none());
            let snapshot = state.entities.snapshot(long_id.unwrap()).unwrap();
            assert_eq!(
                snapshot
                    .private_payload
                    .downcast_ref::<drops::DropEntityPayload>()
                    .unwrap()
                    .created_unix_ms,
                now - 700_000
            );
            assert!(!drops::has_expired(&state.entities, now));
            assert!(drops::has_expired(
                &state.entities,
                now - 700_000 + 3_600_000
            ));
        }
        state.last_expiry_scan = Instant::now() - Duration::from_secs(1);
        gameplay::serve(state, |address| {
            let client =
                crate::client::connect_catalog_probe(&address.to_string(), 0xd201).unwrap();
            assert_eq!(client.fingerprint(), catalog.fingerprint());
            assert_eq!(
                client.drop_policy(client.item_by_key("demo:long").unwrap()),
                catalog.drop_policy(long)
            );
            let mut peer = TcpStream::connect(address).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            protocol::write_client(
                &mut peer,
                &ClientMessage::Hello {
                    name: "drop-policy".into(),
                    profile: 0xd202,
                    content_fingerprint: catalog.fingerprint(),
                },
            )
            .unwrap();
            let (fingerprint, _) = receive_content_manifest(&mut peer);
            protocol::write_client(&mut peer, &ClientMessage::ContentReady { fingerprint })
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                assert!(
                    Instant::now() < deadline,
                    "committed custom-lifetime drops not published"
                );
                if let ServerMessage::Drops { items, .. } =
                    protocol::read_server_with_catalog(&mut peer, &catalog).unwrap()
                    && items.iter().any(|drop| drop.item == long)
                    && items.iter().all(|drop| drop.item != short)
                {
                    assert_eq!(
                        items
                            .iter()
                            .filter(|drop| drop.item == long)
                            .map(|drop| u32::from(drop.count))
                            .sum::<u32>(),
                        129
                    );
                    assert!(items.iter().all(|drop| drop.count <= 128));
                    break;
                }
            }
        });
    }
    let manifest = std::fs::read(fixture.0.join("save/content.map")).unwrap();
    fixture.package(
        "demo",
        CONTENT,
        &source.replace("lifetime_ms=3600000", "lifetime_ms=3600001"),
    );
    assert!(
        fixture.open().is_err(),
        "policy is saved item identity, never a silent world override"
    );
    assert_eq!(
        std::fs::read(fixture.0.join("save/content.map")).unwrap(),
        manifest
    );
}

#[test]
fn default_policy_preserves_bundle_and_custom_policy_combines_with_player_rules() {
    let fixture = Fixture::new();
    fixture.package("demo", CONTENT, TOKEN);
    let baseline = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    fixture.package("demo", CONTENT, "return function(h) h.register_item('demo:token', 'Token', 'bloxgloom:stone', {drop_policy={gravity=24, terminal_speed=30, radius=0.18, pickup_range=2.25, merge_range=1, lifetime_ms=600000}}) end");
    let explicit = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    assert_eq!(
        explicit.catalog().fingerprint(),
        baseline.catalog().fingerprint()
    );
    assert_eq!(
        explicit.client_bundle.as_ref().unwrap().bytes(),
        baseline.client_bundle.as_ref().unwrap().bytes()
    );
    fixture.package("demo", CONTENT, &format!("return function(h)
        h.register_player_rules('demo:small', 1, {})
        h.register_item('demo:token', 'Token', 'bloxgloom:stone', {{drop_size='small', drop_animation={{pickup_arc=1}}, drop_policy={{lifetime_ms=2000}}}})
        end", player::FIELDS));
    let startup = fixture.startup(Arc::new(Catalog::builtins())).unwrap();
    let bundle = startup.client_bundle.as_ref().unwrap();
    assert!(bundle.bytes().starts_with(b"BGCLIENT\x15"));
    let client = bundle.session_catalog().unwrap();
    assert_eq!(client.fingerprint(), startup.catalog().fingerprint());
    assert_eq!(
        client.drop_policy(client.item_by_key("demo:token").unwrap()),
        DropPolicy {
            lifetime_ms: 2_000,
            ..Default::default()
        }
    );
}

#[test]
fn invalid_luau_drop_policy_is_fatal_even_when_caught_before_world_open() {
    for value in [
        "false",
        "{radius=0.5}",
        "{gravity=0/0}",
        "{pickup_range=9}",
        "{merge_range=-1}",
        "{lifetime_ms=999}",
        "{lifetime_ms=1000.5}",
        "{terminal_speed=61}",
        "{unknown=1}",
    ] {
        let fixture = Fixture::new();
        fixture.package("demo", CONTENT, &format!("return function(h) pcall(function() h.register_item('demo:token', 'Token', 'bloxgloom:stone', {{drop_policy={value}}}) end) end"));
        assert!(fixture.open().is_err(), "invalid policy accepted: {value}");
        assert!(!fixture.0.join("save").exists());
    }
}
