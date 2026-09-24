use super::*;

#[test]
fn checkpoint_generation_changes_with_serialized_drop_state() {
    let mut drops = Drops::new();
    let empty = drops.snapshot_bytes().unwrap();
    assert!(drops.matches_checkpoint_generation(&empty));
    assert!(!drops.matches_checkpoint_generation(&empty[..10]));

    let spawn = drops
        .plan_spawns(&[([0.0, 12.0, 0.0], crate::world::STONE, 1, Duration::ZERO)])
        .unwrap();
    drops.apply_plan(&spawn).unwrap();
    assert!(!drops.matches_checkpoint_generation(&empty));

    let current = drops.snapshot_bytes().unwrap();
    assert!(drops.matches_checkpoint_generation(&current));
}
