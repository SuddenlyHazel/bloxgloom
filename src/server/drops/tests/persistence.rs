use super::*;
use std::{fs, time::Duration};

#[test]
fn drops_survive_restart_and_are_still_collectible() {
    let root = temp_root("bloxgloom-drops");
    fs::create_dir_all(&root).unwrap();
    let mut drops = Drops::open(&root).unwrap();
    drops.spawn([1.0, 2.0, 3.0], 2, 100, Duration::ZERO);
    drops.spawn([1.0, 2.0, 3.0], 2, 50, Duration::ZERO);
    assert_eq!(drops.entries.len(), 2);
    drops.save().unwrap();
    let mut loaded = Drops::open(&root).unwrap();
    assert_eq!(
        loaded
            .entries
            .values()
            .map(|entry| entry.item.count)
            .sum::<u16>(),
        150
    );
    assert_spatial_members_match_entries(&loaded);
    let item = loaded.pickup_candidates([1.0, 2.0, 3.0])[0];
    loaded.take(item.id, item.count);
    loaded.save().unwrap();
    assert_eq!(Drops::open(&root).unwrap().entries.len(), 1);
    let mut bytes = fs::read(root.join("drops.bin")).unwrap();
    bytes[HEADER] ^= 1;
    fs::write(root.join("drops.bin"), bytes).unwrap();
    assert!(Drops::open(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ancient_persisted_drop_stays_expired_and_is_not_a_merge_target() {
    let root = temp_root("bloxgloom-drop-ancient");
    fs::create_dir_all(&root).unwrap();
    let mut drops = Drops::open(&root).unwrap();
    drops.spawn([1.0, 2.0, 3.0], 2, 10, Duration::ZERO);
    drops.entries.get_mut(&1).unwrap().created_unix_ms = 1;
    drops.save().unwrap();

    let loaded = Drops::open(&root).unwrap();
    assert!(loaded.pickup_candidates([1.0, 2.0, 3.0]).is_empty());
    assert_eq!(loaded.plan_expired(8).changes[0].id, 1);
    let plan = loaded
        .plan_spawn([1.0, 2.0, 3.0], 2, 5, Duration::ZERO)
        .unwrap();
    assert_eq!(plan.changes.len(), 1);
    assert_eq!(plan.changes[0].id, 2, "expired entry was merged");
    assert!(plan.changes[0].before.is_empty());
    fs::remove_dir_all(root).unwrap();
}
