//! Presentation-only motion for authoritative world drops and pickup events.
use crate::content::Catalog;
use crate::protocol::DroppedItem;
use crate::render::VisualDrop;
use bloxgloom_host_api::content::DropAnimation;
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

const POSITION_BLEND: f32 = 0.08;

struct PickupFlight {
    start: VisualDrop,
    started: Instant,
    animation: DropAnimation,
}

pub(crate) struct DropAnimator {
    items: Vec<DroppedItem>,
    previous_positions: HashMap<u64, Vec3>,
    snapshot_at: Instant,
    pickups: Vec<PickupFlight>,
    catalog: Arc<Catalog>,
}

impl DropAnimator {
    pub(crate) fn new(now: Instant, catalog: Arc<Catalog>) -> Self {
        Self {
            items: Vec::new(),
            previous_positions: HashMap::new(),
            snapshot_at: now,
            pickups: Vec::new(),
            catalog,
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
            let animation = self.catalog.drop_animation(item.item);
            let start = self
                .items
                .iter()
                .find(|live| live.id == item.id)
                .map(|live| {
                    let age = live.age_ms as f32 / 1000.0
                        + now.duration_since(self.snapshot_at).as_secs_f32();
                    let mut visual = live_visual(live, age, animation);
                    visual.center += self.position_for(live, now) - Vec3::from_array(live.position);
                    visual
                })
                .unwrap_or_else(|| live_visual(&item, item.age_ms as f32 / 1000.0, animation));
            if let Some(live) = self.items.iter_mut().find(|live| live.id == item.id) {
                live.count = live.count.saturating_sub(item.count);
            }
            self.pickups.push(PickupFlight {
                start,
                started: now,
                animation,
            });
        }
        self.items.retain(|item| item.count > 0);
        if self.pickups.len() > 256 {
            self.pickups.drain(..self.pickups.len() - 256);
        }
    }

    pub(crate) fn visuals(&mut self, now: Instant, player: Vec3) -> Vec<VisualDrop> {
        self.pickups.retain(|flight| {
            now.duration_since(flight.started).as_secs_f32() < flight.animation.pickup_duration
        });
        let mut result = Vec::with_capacity(self.items.len() + self.pickups.len());
        let elapsed_ms = now
            .duration_since(self.snapshot_at)
            .as_millis()
            .min(u32::MAX as u128);
        for item in &self.items {
            let age = (u128::from(item.age_ms) + elapsed_ms) as f32 / 1000.0;
            let mut visual = live_visual(item, age, self.catalog.drop_animation(item.item));
            visual.center += self.position_for(item, now) - Vec3::from_array(item.position);
            result.push(visual);
        }
        let target = player + Vec3::new(0.0, 1.25, 0.0);
        for flight in &self.pickups {
            let t = (now.duration_since(flight.started).as_secs_f32()
                / flight.animation.pickup_duration)
                .clamp(0.0, 1.0);
            let eased = t * t * (3.0 - 2.0 * t);
            result.push(VisualDrop {
                item: flight.start.item,
                center: flight.start.center.lerp(target, eased)
                    + Vec3::Y * (flight.animation.pickup_arc * (std::f32::consts::PI * t).sin()),
                angle: flight.start.angle + t * flight.animation.pickup_turn,
                scale: flight.start.scale * (1.0 - eased).max(0.03),
                light: Default::default(),
            });
        }
        result
    }
}

fn live_visual(item: &DroppedItem, age: f32, animation: DropAnimation) -> VisualDrop {
    let phase = (item.id.wrapping_mul(0x9e37_79b9) >> 32) as f32 * 0.000_000_001;
    let pop = (age / animation.pop_duration).clamp(0.0, 1.0);
    let ease = pop * pop * (3.0 - 2.0 * pop);
    let lift = 0.14 * ease
        + animation.pop_height * (std::f32::consts::PI * pop).sin()
        + animation.hover_amplitude * (age * animation.hover_speed + phase).sin() * ease;
    VisualDrop {
        item: item.item,
        center: Vec3::from_array(item.position) + Vec3::Y * lift,
        angle: age * animation.spin_speed + phase,
        scale: 0.76 + 0.24 * ease,
        light: Default::default(),
    }
}

#[cfg(test)]
mod tests;
