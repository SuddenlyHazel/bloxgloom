use super::*;
use std::time::{Duration, Instant};

pub(crate) fn example() -> (Arc<ClientBundle>, Catalog, Stack) {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/item-visuals/packages");
    let server = crate::server::package_catalog_for_preview(Catalog::builtins(), &root).unwrap();
    let bundle = crate::server::package_bundle_for_preview(&root).unwrap();
    assert!(bundle.bytes().starts_with(b"BGCLIENT\x2f"));
    let local = bundle.session_catalog().unwrap();
    assert_eq!(local.fingerprint(), server.fingerprint());
    let catalog = crate::content::ContentManifest::from_catalog(&server)
        .resolve_catalog(&local)
        .unwrap();
    let startup = crate::client::startup::prepare(Arc::clone(&bundle)).unwrap();
    catalog
        .item_visuals
        .bind(Arc::clone(&bundle), &startup.item_visual_handlers, &catalog)
        .unwrap();
    let stack = Stack::new(catalog.item_by_key("visual:cell").unwrap(), 1);
    (bundle, catalog, stack)
}
pub(crate) fn wait(catalog: &Catalog, stack: &Stack) -> Arc<Visual> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(visual) = catalog.item_visuals.visual(stack) {
            return visual;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not return item visual"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn item_visuals_delivered_icons_and_stack_callbacks_preserve_catalog_and_inventory() {
    let (_bundle, catalog, mut stack) = example();
    let original = stack.clone();
    let fingerprint = catalog.fingerprint();
    assert_eq!(catalog.item_icon(stack.item).unwrap().rows[2], ".x..x.");
    let small = wait(&catalog, &stack);
    assert_eq!(small.drop_scale, 0.5 + 1. / 128.);
    assert_eq!(
        small.icon.as_ref().unwrap().palette[0].1,
        [0.95, 0.55, 0.15, 1.]
    );
    assert_eq!(stack, original);
    stack.count = 128;
    let full = wait(&catalog, &stack);
    assert_eq!(full.drop_scale, 1.5);
    assert_eq!(
        full.icon.as_ref().unwrap().palette[0].1,
        [0.2, 0.9, 0.3, 1.]
    );
    let component = Stack::with_components(stack.item, 1, 1, vec![255]).unwrap();
    let charged = wait(&catalog, &component);
    assert_eq!(
        charged.icon.as_ref().unwrap().palette[0].1,
        [0.2, 0.5, 1., 1.]
    );
    assert_eq!(
        catalog.fingerprint(),
        fingerprint,
        "client presentation never changes authoritative identity"
    );
}

#[test]
fn item_visuals_readonly_inputs_bounded_replies_and_infinite_callbacks_fail() {
    let (bundle, _catalog, stack) = example();
    // Clone canonical artifact sources through unique temporary package fixtures.
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-item-visual-errors-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("visual/client")).unwrap();
    std::fs::create_dir_all(root.join("visual/server")).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/item-visuals/packages/visual");
    for name in ["package.txt", "server/main.luau", "client/startup.luau"] {
        std::fs::copy(fixture.join(name), root.join("visual").join(name)).unwrap();
    }
    for source in [
        "return function(s) s.count=2; return {} end",
        "return function(s) return {drop_scale=2} end",
        "return function(s) return {icon={rows={'z'},palette={}}} end",
        "return function(s) return {world_edit=1} end",
        "return function(s) while true do end end",
    ] {
        std::fs::write(root.join("visual/client/cell.luau"), source).unwrap();
        let altered = crate::server::package_bundle_for_preview(&root).unwrap();
        assert!(
            execution::run(altered, "visual:cell", "visual:cell", &Key::new(&stack)).is_err(),
            "accepted {source}"
        );
    }
    assert!(execution::run(bundle, "visual:cell", "visual:cell", &Key::new(&stack)).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn item_visuals_cache_deduplicates_pending_and_bounds_entries() {
    let cache = Cache::default();
    let (sender, receiver) = mpsc::sync_channel(QUEUE);
    let mut state = cache.0.lock().unwrap();
    state
        .handlers
        .insert(ItemId(1), ("test:cell".into(), "test:cell".into()));
    state.sender = Some(sender);
    drop(state);
    let stack = Stack::new(ItemId(1), 1);
    for _ in 0..100 {
        assert!(cache.visual(&stack).is_none());
    }
    assert_eq!(receiver.try_iter().count(), 1);
    for i in 0..MAX_ENTRIES + 50 {
        let stack =
            Stack::with_components(ItemId(1), 1, 1, (i as u32).to_le_bytes().to_vec()).unwrap();
        let _ = cache.visual(&stack);
        for key in receiver.try_iter() {
            cache
                .0
                .lock()
                .unwrap()
                .entries
                .get_mut(&key)
                .unwrap()
                .pending = false;
        }
    }
    assert_eq!(cache.0.lock().unwrap().entries.len(), MAX_ENTRIES);
}

#[test]
fn item_visuals_drop_animator_applies_only_client_presentation_scale() {
    let (_bundle, catalog, stack) = example();
    let mut full = stack.clone();
    full.count = 128;
    wait(&catalog, &stack);
    wait(&catalog, &full);
    let catalog = Arc::new(catalog);
    let now = Instant::now();
    let mut animator = crate::client::drops::DropAnimator::new(now, catalog);
    let item = crate::protocol::DroppedItem {
        id: 77,
        item: stack.item,
        count: 1,
        position: [0., 0., 0.],
        age_ms: 1000,
    };
    animator.snapshot(vec![item], now);
    let small = animator.visuals(now, glam::Vec3::ZERO)[0];
    animator.snapshot(
        vec![crate::protocol::DroppedItem { count: 128, ..item }],
        now,
    );
    let large = animator.visuals(now, glam::Vec3::ZERO)[0];
    assert_eq!(small.center, large.center);
    assert_eq!(small.angle, large.angle);
    assert!((large.scale / small.scale - 1.5 / (0.5 + 1. / 128.)).abs() < 0.00001);
    assert_eq!(
        item.count, 1,
        "presentation never edits authoritative snapshot values"
    );
}

#[test]
fn item_visuals_bundle_rejects_nonfinite_icon_payloads_and_missing_items() {
    use sha2::{Digest, Sha256};
    let (bundle, _, _) = example();
    let mut bytes = bundle.bytes().to_vec();
    let len = bytes.len();
    bytes[len - 4..].copy_from_slice(&f32::NAN.to_le_bytes());
    let key = crate::server::client_bundle::CacheKey::from_bytes(Sha256::digest(&bytes).into());
    assert!(ClientBundle::decode_verify(&bytes, key).is_err());
    let mut bytes = bundle.bytes().to_vec();
    let index = bytes
        .windows(11)
        .rposition(|b| b == b"visual:cell")
        .unwrap();
    bytes[index..index + 11].copy_from_slice(b"visual:fake");
    let key = crate::server::client_bundle::CacheKey::from_bytes(Sha256::digest(&bytes).into());
    assert!(ClientBundle::decode_verify(&bytes, key).is_err());
}
