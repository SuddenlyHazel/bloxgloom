use super::persistence::{HEADER, TEMP_SEQUENCE};
use super::*;
use std::collections::BTreeSet;
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
        Entry::new(
            id,
            position,
            DropEntityPayload::new(
                Stack::new(ItemId::new(u32::from(item)), count),
                created_unix_ms,
                pickup_delay,
            ),
            0.0,
            age,
            age_since,
        ),
    );
    drops.spatial.insert(id, position);
    drops.expiry.insert(id, age, age_since);
    if age < LIFETIME {
        drops.active.insert(id);
    }
    let previous_next = drops.next_id;
    drops.next_id = drops.next_id.max(id.saturating_add(1));
    if drops.next_id != previous_next {
        drops.allocator_dirty = true;
    }
    drops.index_insert(id, position);
}

pub(super) fn assert_spatial_members_match_entries(drops: &Drops) {
    assert_eq!(drops.entries.len(), drops.spatial.len());
    assert_eq!(drops.entries.len(), drops.expiry.len());
    let member_total: usize = drops.chunk_members.values().map(BTreeSet::len).sum();
    assert_eq!(drops.entries.len(), member_total);
    for (&id, entry) in &drops.entries {
        assert!(drops.spatial.contains(id));
        assert!(drops.expiry.contains(id));
        let candidates = drops.spatial.query_aabb(entry.position, entry.position);
        assert!(candidates.contains(&id));
        let chunk = super::chunk_of(entry.position);
        assert!(
            drops
                .chunk_members
                .get(&chunk)
                .is_some_and(|members| members.contains(&id))
        );
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
mod pins;
mod planning_pickup;
mod shards;
mod spatial_physics;
