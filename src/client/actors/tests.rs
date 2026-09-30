use super::*;
fn avatar(x: f32) -> VisualAvatar {
    VisualAvatar {
        animation: Default::default(),
        model: AvatarModel::Registered(crate::content::MOSSBUN_ENTITY_TYPE),
        pose: [0.0; 4],
        airborne: false,
        id: 1,
        position: glam::Vec3::new(x, 80.0, 0.5),
        cosmetics: [0; 4],
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }
}

#[test]
fn interpolation_moves_between_samples_and_freezes_without_extrapolation() {
    let now = Instant::now();
    let mut track = Track::new(avatar(0.0), now);
    track.update(avatar(0.1), now + STEP);
    track.update(avatar(0.2), now + STEP * 2);
    let halfway = track.update(avatar(0.2), now + Duration::from_millis(100));
    assert!((halfway.position.x - 0.05).abs() < 0.001);
    let stopped = track.update(avatar(0.2), now + Duration::from_secs(1));
    assert_eq!(stopped.position.x, 0.2);
    for i in 11..30 {
        track.update(avatar(0.2), now + Duration::from_millis(i * 100));
    }
    assert!(
        track.gait < 0.001,
        "feet stop despite a stale walking intent"
    );
}

#[test]
fn teleport_despawn_and_reappearance_reset_history() {
    let now = Instant::now();
    let mut animator = ActorAnimator::default();
    animator.present(&mut [avatar(0.0)], now);
    let mut jumped = [avatar(50.0)];
    animator.present(&mut jumped, now + STEP);
    assert_eq!(jumped[0].position.x, 50.0);
    animator.present(&mut [], now + STEP * 2);
    assert!(animator.tracks.is_empty());
    let mut appeared = [avatar(1.0)];
    animator.present(&mut appeared, now + STEP * 3);
    assert_eq!(appeared[0].position.x, 1.0);
}

#[test]
fn landing_animation_follows_delayed_ground_contact_and_is_visual_only() {
    let now = Instant::now();
    let mut falling = avatar(0.5);
    falling.airborne = true;
    falling.position.y = 80.1;
    let mut track = Track::new(falling, now);
    let landed = avatar(0.5);
    let early = track.update(landed, now + STEP);
    assert!(early.airborne);
    let shown = track.update(landed, now + STEP + DELAY);
    assert!(!shown.airborne);
    assert!(shown.pose[3] > 0.0);
    assert_eq!(shown.position.y, 80.0);
    assert_eq!(landed.pose, [0.0; 4]);
}
