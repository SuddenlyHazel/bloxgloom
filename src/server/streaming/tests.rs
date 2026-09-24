use super::*;

#[test]
fn drop_replication_ignores_age_but_not_authoritative_changes() {
    let original = DroppedItem {
        id: 1,
        item: 2,
        count: 3,
        position: [1.0, 2.0, 3.0],
        age_ms: 20,
    };
    let mut changed = original;
    changed.age_ms = 40;
    assert!(same_drop_positions(&[original], &[changed]));
    changed.position[1] -= 0.5;
    assert!(!same_drop_positions(&[original], &[changed]));
}
