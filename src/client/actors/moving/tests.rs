use super::*;
use crate::render::{AvatarModel, MovingVisual};

fn avatar(x: f32, tick: u64, revision: u64) -> VisualAvatar {
    let mut avatar = crate::client::actors::tests::avatar(x);
    avatar.model = AvatarModel::Moving(crate::content::MOSSBUN_ENTITY_TYPE);
    avatar.motion = Some(MovingVisual {
        tick,
        revision,
        orientation: glam::Quat::IDENTITY.to_array(),
        velocity: [1.0, 0.0, 0.0],
        stopped: false,
    });
    avatar
}

#[test]
fn committed_ticks_interpolate_orientation_without_gait_or_extrapolation() {
    let now = Instant::now();
    let mut track = Track::new(avatar(0.0, 0, 0), now);
    let mut next = avatar(0.2, 2, 1);
    next.motion.as_mut().unwrap().orientation =
        glam::Quat::from_rotation_z(std::f32::consts::PI / 2.0).to_array();
    track.update(next, now + Duration::from_millis(40));
    let frame = track.update(next, now + Duration::from_millis(100));
    assert!((frame.position.x - 0.1).abs() < 0.0001);
    let facing = glam::Quat::from_array(frame.motion.unwrap().orientation) * glam::Vec3::X;
    assert!((facing.x - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.0001);
    assert_eq!(frame.pose, [0.0; 4]);
    assert_eq!(frame.character_pose, [0.0; 3]);
    assert_eq!(
        track.update(next, now + Duration::from_secs(1)).position,
        next.position
    );
}

#[test]
fn impact_corrects_immediately_and_stale_motion_cannot_resurrect_flight() {
    let now = Instant::now();
    let mut track = Track::new(avatar(0.0, 0, 0), now);
    let mut stopped = avatar(0.1, 1, 1);
    stopped.motion.as_mut().unwrap().stopped = true;
    stopped.motion.as_mut().unwrap().velocity = [0.0; 3];
    assert_eq!(
        track
            .update(stopped, now + Duration::from_millis(20))
            .position,
        stopped.position
    );
    let stale = track.update(avatar(0.5, 0, 0), now + Duration::from_millis(40));
    assert_eq!(stale.position, stopped.position);
    assert!(stale.motion.unwrap().stopped);
    let future_tick_stale_revision =
        track.update(avatar(0.9, 2, 0), now + Duration::from_millis(60));
    assert_eq!(future_tick_stale_revision.position, stopped.position);
}

#[test]
fn model_replacement_and_removal_drop_retained_motion_history() {
    let now = Instant::now();
    let mut animator = crate::client::actors::ActorAnimator::default();
    animator.present(&mut [avatar(0.0, 0, 0)], now);
    assert_eq!(animator.moving.len(), 1);
    animator.present(&mut [], now + Duration::from_millis(20));
    assert!(animator.moving.is_empty());
    let mut rejoined = [avatar(1.0, 0, 0)];
    animator.present(&mut rejoined, now + Duration::from_millis(40));
    assert_eq!(rejoined[0].position.x, 1.0);
    animator.present(
        &mut [crate::client::actors::tests::avatar(1.0)],
        now + Duration::from_millis(60),
    );
    assert!(animator.moving.is_empty());
}

#[test]
fn codec_normalization_tolerance_cannot_break_quaternion_interpolation() {
    let now = Instant::now();
    let mut first = avatar(0.0, 0, 0);
    first.motion.as_mut().unwrap().orientation[3] = 1.0003;
    let mut track = Track::new(first, now);
    let mut next = avatar(0.1, 2, 1);
    next.motion.as_mut().unwrap().orientation[3] = 1.0003;
    let shown = track.update(next, now + Duration::from_millis(40));
    assert!(glam::Quat::from_array(shown.motion.unwrap().orientation).is_normalized());
}
