use super::*;

mod native;

#[test]
fn moving_launch_fences_complete_body_and_uses_final_terrain_overlay() {
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-motion-launch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut world = World::new(7, path.clone()).unwrap();
    let catalog = world.catalog_arc();
    // Center is in air, but the body crosses a chunk seam into a solid cell.
    world.edit(15, 300, 0, crate::world::AIR).unwrap();
    world.edit(16, 300, 0, crate::world::STONE).unwrap();
    let position = [15.95, 300.5, 0.5];
    let half = [0.12; 3];
    let mut requested = Vec::new();
    assert!(
        validate_spawn_volume(
            &mut world,
            &mut TerrainReads::default(),
            &mut requested,
            &catalog,
            &[],
            half,
            position
        )
        .is_err()
    );
    let mut reads = TerrainReads::default();
    validate_spawn_volume(
        &mut world,
        &mut reads,
        &mut requested,
        &catalog,
        &[(16, 300, 0, crate::world::AIR)],
        half,
        position,
    )
    .unwrap();
    assert!(!reads.is_empty() && reads.is_current());
    world.edit(15, 300, 0, crate::world::STONE).unwrap();
    assert!(
        !reads.is_current(),
        "launch air must fence a later obstruction"
    );
    world.edit(15, 300, 0, crate::world::AIR).unwrap();
    world.edit(16, 300, 0, crate::world::AIR).unwrap();
    assert!(
        validate_spawn_volume(
            &mut world,
            &mut TerrainReads::default(),
            &mut requested,
            &catalog,
            &[(16, 300, 0, crate::world::STONE)],
            half,
            position
        )
        .is_err()
    );
    drop(world);
    std::fs::remove_dir_all(path).unwrap();
}
