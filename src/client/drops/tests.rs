use super::*;
use std::time::Duration;

fn item(age_ms: u32) -> DroppedItem {
    DroppedItem {
        id: 7,
        item: crate::items::ItemId::new(2),
        count: 4,
        components: None,
        position: [1.0, 2.0, 3.0],
        age_ms,
    }
}

#[test]
fn pop_uses_server_age_and_pickup_flies_before_disappearing() {
    let now = Instant::now();
    let mut animator = DropAnimator::new(now, Arc::new(crate::content::Catalog::builtins()));
    animator.snapshot(vec![item(10)], now);
    let first = animator.visuals(now, Vec3::ZERO);
    let apex = animator.visuals(now + Duration::from_millis(260), Vec3::ZERO);
    assert!(apex[0].center.y > first[0].center.y + 0.5);
    animator.snapshot(vec![item(2000)], now);
    let old = animator.visuals(now, Vec3::ZERO);
    assert!(old[0].center.y < apex[0].center.y - 0.4);
    animator.picked_up(vec![item(2000)], now);
    let first_flight = animator.visuals(now, Vec3::ZERO);
    assert_eq!(first_flight.len(), 1);
    assert_eq!(first_flight[0].center, old[0].center);
    assert_eq!(first_flight[0].angle, old[0].angle);
    assert_eq!(first_flight[0].scale, old[0].scale);
    let mid = animator.visuals(now + Duration::from_millis(200), Vec3::ZERO);
    assert!(mid[0].scale < first_flight[0].scale);
    assert!(mid[0].center.distance(Vec3::new(0.0, 1.25, 0.0)) < 2.0);
    assert!(
        animator
            .visuals(now + Duration::from_millis(350), Vec3::ZERO)
            .is_empty()
    );
}

#[test]
fn fresh_snapshot_keeps_spin_phase_for_old_items() {
    let now = Instant::now();
    let mut animator = DropAnimator::new(now, Arc::new(crate::content::Catalog::builtins()));
    animator.snapshot(vec![item(90_000)], now);
    let before = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0].angle;
    animator.snapshot(vec![item(90_100)], now + Duration::from_millis(100));
    let after = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0].angle;
    assert!((before - after).abs() < 0.001);
}

#[test]
fn moving_drop_blends_between_authoritative_positions() {
    let now = Instant::now();
    let mut animator = DropAnimator::new(now, Arc::new(crate::content::Catalog::builtins()));
    let first = item(2000);
    animator.snapshot(vec![first.clone()], now);
    let mut next = first;
    next.position[1] -= 1.0;
    animator.snapshot(vec![next], now + Duration::from_millis(20));
    let visual_start = animator.visuals(now + Duration::from_millis(20), Vec3::ZERO)[0].center;
    let visual_mid = animator.visuals(now + Duration::from_millis(60), Vec3::ZERO)[0].center;
    let visual_end = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0].center;
    assert!(visual_start.y > visual_mid.y && visual_mid.y > visual_end.y);
}

#[test]
fn sized_drop_keeps_its_preset_through_pickup_flight_without_changing_motion() {
    use bloxgloom_host_api::content::{Components, DropSize, Item};
    let mut catalog = crate::content::Catalog::builtins();
    catalog
        .public_item(&Item {
            key: "test:sized".into(),
            name: "Sized".into(),
            swatch: [1.0; 4],
            texture: "bloxgloom:stone".into(),
            placeable: None,
            sprite: true,
            drop_size: DropSize::Small,
            drop_animation: Default::default(),
            drop_policy: Default::default(),
            components: Components::None,
        })
        .unwrap();
    let now = Instant::now();
    let mut animator = DropAnimator::new(now, Arc::new(catalog.clone()));
    let mut drop = item(2000);
    drop.item = catalog.item_by_key("test:sized").unwrap();
    animator.snapshot(vec![drop.clone()], now);
    let live = animator.visuals(now, Vec3::ZERO)[0];
    assert_eq!(live.presentation_scale(&catalog), live.scale * 0.75);
    animator.picked_up(vec![drop], now);
    let first = animator.visuals(now, Vec3::ZERO)[0];
    assert_eq!(first.center, live.center);
    assert_eq!(
        first.presentation_scale(&catalog),
        live.presentation_scale(&catalog)
    );
    let mid = animator.visuals(now + Duration::from_millis(170), Vec3::ZERO)[0];
    assert_eq!(mid.presentation_scale(&catalog), mid.scale * 0.75);
    assert!(mid.presentation_scale(&catalog) < first.presentation_scale(&catalog));
}

#[test]
fn authored_motion_uses_server_age_and_continues_into_partial_pickup() {
    use bloxgloom_host_api::content::{Components, DropSize, Item};
    let mut catalog = Catalog::builtins();
    let animation = DropAnimation {
        pop_duration: 1.0,
        pop_height: 1.5,
        hover_amplitude: 0.0,
        hover_speed: 0.0,
        spin_speed: 4.0,
        pickup_duration: 0.8,
        pickup_arc: 1.2,
        pickup_turn: 8.0,
    };
    catalog
        .public_item(&Item {
            key: "test:animated".into(),
            name: "Animated".into(),
            swatch: [1.0; 4],
            texture: "bloxgloom:stone".into(),
            placeable: None,
            sprite: true,
            drop_size: DropSize::Normal,
            drop_animation: animation,
            drop_policy: Default::default(),
            components: Components::None,
        })
        .unwrap();
    let now = Instant::now();
    let mut animator = DropAnimator::new(now, Arc::new(catalog.clone()));
    let mut drop = item(500);
    drop.item = catalog.item_by_key("test:animated").unwrap();
    animator.snapshot(vec![drop.clone()], now);
    let live = animator.visuals(now, Vec3::ZERO)[0];
    assert!((live.center.y - 3.57).abs() < 0.01);
    assert!(
        (live.angle - (2.0 + ((drop.id.wrapping_mul(0x9e37_79b9) >> 32) as f32 * 0.000_000_001)))
            .abs()
            < 0.001
    );
    let mut partial = drop.clone();
    partial.count = 2;
    animator.picked_up(vec![partial], now);
    let first = animator.visuals(now, Vec3::ZERO);
    assert_eq!(first.len(), 2);
    assert_eq!(first[1].center, live.center);
    assert_eq!(first[1].angle, live.angle);
    assert_eq!(first[1].scale, live.scale);
    let mid = animator.visuals(now + Duration::from_millis(400), Vec3::ZERO);
    assert_eq!(mid.len(), 2);
    assert!((mid[1].angle - live.angle - 4.0).abs() < 0.001);
    assert!(mid[1].center.y > live.center.y);
    assert_eq!(
        animator
            .visuals(now + Duration::from_millis(800), Vec3::ZERO)
            .len(),
        1
    );
    let before = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0];
    drop.age_ms = 600;
    animator.snapshot(vec![drop], now + Duration::from_millis(100));
    let after = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0];
    assert!((before.angle - after.angle).abs() < 0.001);
    assert!((before.center.y - after.center.y).abs() < 0.001);
}
