use super::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

#[test]
fn tiny_motion_advances_the_checkpoint_generation() {
    let root = std::env::temp_dir().join(format!(
        "bloxgloom-tiny-drop-motion-{}-{}",
        std::process::id(),
        NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let mut world = World::new(19, root.clone()).unwrap();
    world.get_block(1, 511, 1).unwrap();

    let mut drops = Drops::new();
    drops.spawn([1.0, 512.05, 1.0], crate::world::STONE, 1, Duration::ZERO);
    let old = drops.snapshot_bytes().unwrap();
    let old_y = drops.entries[&1].item.position[1];
    let step = drops.step(&world, Duration::from_micros(1_700));
    let new_y = drops.entries[&1].item.position[1];
    assert_ne!(old_y.to_bits(), new_y.to_bits());
    assert!((old_y - new_y).abs() < 0.000_1);
    assert!(step.moved);
    assert!(!drops.matches_checkpoint_generation(&old));
    fs::remove_dir_all(root).unwrap();
}
