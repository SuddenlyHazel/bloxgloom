use super::{Settings, Source};
use glam::Vec3;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Slot {
    pub source: Option<Source>,
    pub weight: f32,
    pub initialized: bool,
}

pub(super) fn select(
    slots: &mut [Slot],
    eye: Vec3,
    candidates: &[Source],
    settings: Settings,
    dt: f32,
) {
    let mut ranked: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|s| {
            s.position.is_finite()
                && s.range.is_finite()
                && s.range > 0.0
                && s.color.iter().all(|v| v.is_finite() && *v >= 0.0)
                && s.position.distance_squared(eye) <= (settings.range * 2.0).powi(2)
        })
        .map(|mut source| {
            source.range = source.range.max(0.1);
            let retained = slots
                .iter()
                .any(|slot| slot.source.is_some_and(|s| s.position == source.position));
            let score = source.position.distance_squared(eye) * if retained { 0.8 } else { 1.0 };
            (source, score)
        })
        .collect();
    ranked.sort_by(|(a, ad), (b, bd)| {
        ad.total_cmp(bd)
            .then(a.position.x.total_cmp(&b.position.x))
            .then(a.position.y.total_cmp(&b.position.y))
            .then(a.position.z.total_cmp(&b.position.z))
    });
    ranked.dedup_by(|a, b| a.0.position == b.0.position);
    ranked.truncate(settings.count);
    let step = if dt.is_finite() {
        dt.clamp(0.0, 0.25) * 4.0
    } else {
        0.0
    };
    for slot in slots.iter_mut() {
        if let Some(current) = slot.source {
            if let Some((updated, _)) = ranked.iter().find(|(s, _)| s.position == current.position)
            {
                if *updated != current {
                    slot.source = Some(*updated);
                    slot.initialized = false;
                    slot.weight = 0.0;
                }
                if slot.initialized {
                    slot.weight = (slot.weight + step).min(1.0);
                }
            } else {
                slot.weight = (slot.weight - step).max(0.0);
                if slot.weight == 0.0 {
                    *slot = Slot::default();
                }
            }
        }
    }
    for (source, _) in ranked {
        if slots
            .iter()
            .any(|s| s.source.is_some_and(|s| s.position == source.position))
        {
            continue;
        }
        if let Some(slot) = slots.iter_mut().find(|s| s.source.is_none()) {
            slot.source = Some(source);
        }
    }
}
