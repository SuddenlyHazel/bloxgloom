use super::*;

#[test]
fn fast_movement_cannot_accelerate_authored_walk_past_normal_playback() {
    let now = Instant::now();
    let mut animator = ActorAnimator::default();
    let mut player = avatar(0.0);
    player.model = AvatarModel::Player;
    animator.present_with_local(&mut [player], now, Some(player.id));
    let mut frame = [player];
    for step in 1..=25 {
        player.position.x = step as f32 * 0.32;
        frame = [player];
        animator.present_with_local(&mut frame, now + STEP * step, Some(player.id));
    }
    assert!(
        (frame[0].character_pose[0] - 1.25).abs() < 0.01,
        "one second at eight blocks/sec plays at most 1.25 seconds of animation"
    );
}

#[test]
fn predicted_local_player_is_not_delayed_and_faces_the_current_look_heading() {
    let now = Instant::now();
    let mut animator = ActorAnimator::default();
    let mut player = avatar(0.0);
    player.model = AvatarModel::Player;
    animator.present_with_local(&mut [player], now, Some(player.id));
    player.position.x = 0.1;
    player.pose[0] = 1.2;
    let mut frame = [player];
    animator.present_with_local(&mut frame, now + STEP, Some(player.id));
    assert_eq!(frame[0].position, player.position);
    assert_eq!(frame[0].pose[0], player.pose[0]);
    assert!(frame[0].character_pose[0] > 0.0);
    assert!(frame[0].character_pose[2] > 0.0);
    player.pose[0] = -0.7;
    let mut idle = [player];
    animator.present_with_local(&mut idle, now + STEP * 2, Some(player.id));
    assert_eq!(
        idle[0].pose[0], -0.7,
        "turning while idle follows the local look direction"
    );
}
pub(super) fn avatar(x: f32) -> VisualAvatar {
    VisualAvatar {
        motion: None,
        character_pose: [0.0; 4],
        character_look: [0.0; 2],
        character_crouch: 0.0,
        character_tool: None,
        character_recipe: None,
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

#[test]
fn player_walk_blends_from_replicated_distance_then_stops_without_drift() {
    let now = Instant::now();
    let player = |x| VisualAvatar {
        model: AvatarModel::Player,
        ..avatar(x)
    };
    let mut track = Track::new(player(0.0), now);
    for i in 1..15 {
        track.update(player(i as f32 * 0.05), now + STEP * i);
    }
    let walking = track.update(player(0.7), now + STEP * 15);
    assert!(walking.character_pose[0] > 0.0 && walking.character_pose[2] > 0.5);
    for i in 16..80 {
        track.update(player(0.7), now + STEP * i);
    }
    let stopped = track.update(player(0.7), now + STEP * 80);
    assert_eq!(stopped.position, player(0.7).position);
    assert!(stopped.character_pose[2] < 0.001);
    assert!(
        (stopped.pose[0] - std::f32::consts::FRAC_PI_2).abs() < 0.01,
        "idle player must retain movement heading"
    );
    assert!(stopped.character_pose[1] > walking.character_pose[1]);
    let phase = stopped.character_pose[0];
    assert_eq!(
        track.update(player(0.7), now + STEP * 81).character_pose[0],
        phase
    );
    let reset = track.update(player(100.0), now + STEP * 82);
    assert_eq!(reset.character_pose[0], 0.0);
    assert_eq!(reset.character_pose[2], 0.0);
}

#[test]
fn switching_actor_model_cannot_reuse_character_gait_or_old_pose() {
    let now = Instant::now();
    let player = |x| VisualAvatar {
        model: AvatarModel::Player,
        ..avatar(x)
    };
    let mut track = Track::new(player(0.0), now);
    for i in 1..10 {
        track.update(player(i as f32 * 0.1), now + STEP * i);
    }
    assert!(track.gait > 0.0);
    let creature = track.update(avatar(0.9), now + STEP * 10);
    assert_eq!(creature.character_pose, [0.0; 4]);
    let replaced = track.update(player(0.9), now + STEP * 11);
    assert_eq!(replaced.character_pose[0], 0.0);
    assert_eq!(replaced.character_pose[2], 0.0);
}

#[test]
fn ground_speed_blends_walk_and_run_but_stale_or_airborne_motion_decays() {
    let now = Instant::now();
    let player = |x, airborne| VisualAvatar {
        model: AvatarModel::Player,
        airborne,
        ..avatar(x)
    };
    let mut slow = Track::new(player(0.0, false), now);
    let mut fast = Track::new(player(0.0, false), now);
    let mut visual = player(0.0, false);
    for i in 1..=30 {
        let at = now + STEP * i;
        let walk = slow.update_mode(player(i as f32 * 0.08, false), at, true);
        visual = fast.update_mode(player(i as f32 * 0.32, false), at, true);
        assert!(walk.character_pose[3] < 0.001);
    }
    assert!(visual.character_pose[3] > 0.99);
    let frozen = visual.position.x;
    for i in 31..=70 {
        visual = fast.update_mode(player(frozen, false), now + STEP * i, true);
    }
    assert!(visual.character_pose[2] < 0.001 && visual.character_pose[3] < 0.001);
    for i in 71..=110 {
        visual = fast.update_mode(
            player(frozen + (i - 70) as f32 * 0.32, true),
            now + STEP * i,
            true,
        );
    }
    assert!(visual.character_pose[2] < 0.001 && visual.character_pose[3] < 0.001);
}

#[test]
fn crouch_and_head_pitch_ease_and_teleport_resets_the_presentation_history() {
    let now = Instant::now();
    let mut player = VisualAvatar {
        model: AvatarModel::Player,
        ..avatar(0.0)
    };
    let mut track = Track::new(player, now);
    player.character_crouch = 1.0;
    player.character_look = [0.0, 1.2];
    let first = track.update_mode(player, now + STEP, true);
    assert!(first.character_crouch > 0.0 && first.character_crouch < 1.0);
    assert!(first.character_look[1] > 0.0 && first.character_look[1] < 5.0_f32.to_radians());
    for i in 2..=30 {
        track.update_mode(player, now + STEP * i, true);
    }
    player.character_crouch = 0.0;
    player.character_look[1] = 0.0;
    let first = track.update_mode(player, now + STEP * 31, true);
    assert!(first.character_crouch > 0.0 && first.character_crouch < 1.0);
    for i in 32..=70 {
        track.update_mode(player, now + STEP * i, true);
    }
    player.position.x = 100.0;
    let teleported = track.update_mode(player, now + STEP * 71, true);
    assert_eq!(teleported.character_pose[0], 0.0);
    assert_eq!(teleported.character_pose[2], 0.0);
    assert_eq!(teleported.character_pose[3], 0.0);
    assert_eq!(teleported.character_crouch, 0.0);
    assert_eq!(teleported.character_look, [0.0; 2]);
}
