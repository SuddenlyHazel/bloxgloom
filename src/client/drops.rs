//! Presentation-only motion for authoritative world drops and pickup events.
use crate::protocol::DroppedItem;
use crate::render::VisualDrop;
use glam::Vec3;
use std::collections::HashMap;
use std::time::Instant;

const POP: f32 = 0.55;
const PICKUP: f32 = 0.34;
const POSITION_BLEND: f32 = 0.08;

struct PickupFlight {
    start: VisualDrop,
    started: Instant,
}

pub(crate) struct DropAnimator {
    items: Vec<DroppedItem>,
    previous_positions: HashMap<u64, Vec3>,
    snapshot_at: Instant,
    pickups: Vec<PickupFlight>,
}

impl DropAnimator {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            items: Vec::new(),
            previous_positions: HashMap::new(),
            snapshot_at: now,
            pickups: Vec::new(),
        }
    }

    pub(crate) fn snapshot(&mut self, items: Vec<DroppedItem>, now: Instant) {
        let previous = self
            .items
            .iter()
            .map(|item| (item.id, self.position_for(item, now)))
            .collect();
        self.previous_positions = previous;
        self.items = items;
        self.snapshot_at = now;
    }

    fn position_for(&self, item: &DroppedItem, now: Instant) -> Vec3 {
        let target = Vec3::from_array(item.position);
        let Some(start) = self.previous_positions.get(&item.id) else {
            return target;
        };
        let t = (now.duration_since(self.snapshot_at).as_secs_f32() / POSITION_BLEND).min(1.0);
        start.lerp(target, t)
    }

    pub(crate) fn picked_up(&mut self, items: Vec<DroppedItem>, now: Instant) {
        for item in items {
            let start = self
                .items
                .iter()
                .find(|live| live.id == item.id)
                .map(|live| {
                    let age = live.age_ms as f32 / 1000.0
                        + now.duration_since(self.snapshot_at).as_secs_f32();
                    let mut visual = live_visual(live, age);
                    visual.center += self.position_for(live, now) - Vec3::from_array(live.position);
                    visual
                })
                .unwrap_or_else(|| live_visual(&item, item.age_ms as f32 / 1000.0));
            if let Some(live) = self.items.iter_mut().find(|live| live.id == item.id) {
                live.count = live.count.saturating_sub(item.count);
            }
            self.pickups.push(PickupFlight {
                start,
                started: now,
            });
        }
        self.items.retain(|item| item.count > 0);
        if self.pickups.len() > 256 {
            self.pickups.drain(..self.pickups.len() - 256);
        }
    }

    pub(crate) fn visuals(&mut self, now: Instant, player: Vec3) -> Vec<VisualDrop> {
        self.pickups
            .retain(|flight| now.duration_since(flight.started).as_secs_f32() < PICKUP);
        let mut result = Vec::with_capacity(self.items.len() + self.pickups.len());
        let elapsed_ms = now
            .duration_since(self.snapshot_at)
            .as_millis()
            .min(u32::MAX as u128);
        for item in &self.items {
            let age = (u128::from(item.age_ms) + elapsed_ms) as f32 / 1000.0;
            let mut visual = live_visual(item, age);
            visual.center += self.position_for(item, now) - Vec3::from_array(item.position);
            result.push(visual);
        }
        let target = player + Vec3::new(0.0, 1.25, 0.0);
        for flight in &self.pickups {
            let t = (now.duration_since(flight.started).as_secs_f32() / PICKUP).clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            result.push(VisualDrop {
                item: flight.start.item,
                center: flight.start.center.lerp(target, eased)
                    + Vec3::Y * (0.32 * (std::f32::consts::PI * t).sin()),
                angle: flight.start.angle + t * 5.0,
                scale: flight.start.scale * (1.0 - eased).max(0.03),
            });
        }
        result
    }
}

fn live_visual(item: &DroppedItem, age: f32) -> VisualDrop {
    let phase = (item.id.wrapping_mul(0x9e37_79b9) >> 32) as f32 * 0.000_000_001;
    let pop = (age / POP).clamp(0.0, 1.0);
    let ease = pop * pop * (3.0 - 2.0 * pop);
    let lift = 0.14 * ease
        + 0.75 * (std::f32::consts::PI * pop).sin()
        + 0.07 * (age * 2.6 + phase).sin() * ease;
    VisualDrop {
        item: item.item,
        center: Vec3::from_array(item.position) + Vec3::Y * lift,
        angle: age * 2.1 + phase,
        scale: 0.76 + 0.24 * ease,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn item(age_ms: u32) -> DroppedItem {
        DroppedItem {
            id: 7,
            item: 2,
            count: 4,
            position: [1.0, 2.0, 3.0],
            age_ms,
        }
    }

    #[test]
    fn pop_uses_server_age_and_pickup_flies_before_disappearing() {
        let now = Instant::now();
        let mut animator = DropAnimator::new(now);
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
        let mid = animator.visuals(now + Duration::from_millis(200), Vec3::ZERO);
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
        let mut animator = DropAnimator::new(now);
        animator.snapshot(vec![item(90_000)], now);
        let before = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0].angle;
        animator.snapshot(vec![item(90_100)], now + Duration::from_millis(100));
        let after = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0].angle;
        assert!((before - after).abs() < 0.001);
    }

    #[test]
    fn moving_drop_blends_between_authoritative_positions() {
        let now = Instant::now();
        let mut animator = DropAnimator::new(now);
        let first = item(2000);
        animator.snapshot(vec![first], now);
        let mut next = first;
        next.position[1] -= 1.0;
        animator.snapshot(vec![next], now + Duration::from_millis(20));
        let visual_start = animator.visuals(now + Duration::from_millis(20), Vec3::ZERO)[0].center;
        let visual_mid = animator.visuals(now + Duration::from_millis(60), Vec3::ZERO)[0].center;
        let visual_end = animator.visuals(now + Duration::from_millis(100), Vec3::ZERO)[0].center;
        assert!(visual_start.y > visual_mid.y && visual_mid.y > visual_end.y);
    }
}
