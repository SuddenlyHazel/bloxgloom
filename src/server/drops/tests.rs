use super::persistence::{HEADER, TEMP_SEQUENCE};
use super::*;
use std::time::{Duration, Instant};

pub(super) fn temp_root(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

pub(super) const fn item(id: u32) -> ItemId {
    ItemId::new(id)
}

pub(super) fn insert_entry(
    drops: &mut Drops,
    id: u64,
    position: [f32; 3],
    item: u8,
    count: u16,
    age: Duration,
    pickup_delay: Duration,
) {
    let age_since = Instant::now();
    let created_unix_ms = unix_ms().saturating_sub(age.as_millis().min(u64::MAX as u128) as u64);
    drops.entries.insert(
        id,
        Entry {
            item: DroppedItem {
                id,
                item: ItemId::new(u32::from(item)),
                count,
                position,
                age_ms: 0,
            },
            components: None,
            vertical_speed: 0.0,
            age_at_load: age,
            age_since,
            created_unix_ms,
            pickup_delay,
        },
    );
    drops.spatial.insert(id, position);
    drops.expiry.insert(id, age, age_since);
    if age < LIFETIME {
        drops.active.insert(id);
    }
    drops.next_id = drops.next_id.max(id.saturating_add(1));
}

pub(super) fn assert_spatial_members_match_entries(drops: &Drops) {
    assert_eq!(drops.entries.len(), drops.spatial.len());
    assert_eq!(drops.entries.len(), drops.expiry.len());
    for (&id, entry) in &drops.entries {
        assert!(drops.spatial.contains(id));
        assert!(drops.expiry.contains(id));
        let candidates = drops
            .spatial
            .query_aabb(entry.item.position, entry.item.position);
        assert!(candidates.contains(&id));
    }
}

pub(super) fn stable_items(items: &[DroppedItem]) -> Vec<(u64, ItemId, u16, [f32; 3])> {
    items
        .iter()
        .map(|item| (item.id, item.item, item.count, item.position))
        .collect()
}

mod journal_recovery;
mod persistence;
mod planning_pickup;
mod spatial_physics;
