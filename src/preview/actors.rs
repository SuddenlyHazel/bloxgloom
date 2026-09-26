//! Deterministic presentation sequence using the production actor animator.
use crate::{client::actors::ActorAnimator, render::VisualAvatar};
use std::time::{Duration, Instant};

pub(super) fn animate(actors: &mut [VisualAvatar; 3], frame: u32) {
    let original = *actors;
    let now = Instant::now();
    let mut animator = ActorAnimator::default();
    for index in 0..=frame {
        let elapsed = index as f32 / 60.0;
        let sample_time = (elapsed / 0.04).floor() * 0.04;
        *actors = original;
        // Left: walking then stopping. Right: accelerating fall and landing.
        actors[0].position.x += sample_time.min(0.6) * 1.5625;
        actors[0].pose[0] = std::f32::consts::FRAC_PI_2;
        let falling_time = (sample_time - 0.3).max(0.0);
        let height = (2.0 - 10.0 * falling_time * falling_time).max(0.0);
        actors[1].position.y += height;
        actors[1].airborne = height > 0.0;
        animator.present(actors, now + Duration::from_secs_f32(elapsed));
    }
}
