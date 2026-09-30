use super::*;

#[test]
fn teleport_air_reads_are_fenced_and_final_edits_can_open_or_obstruct_destination() {
    let path = std::env::temp_dir().join(format!(
        "bloxgloom-teleport-read-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut world = World::new(7, path.clone()).unwrap();
    world.edit(2, 300, 0, crate::world::AIR).unwrap();
    let operation = PlayerOperation {
        profile: 1,
        session: 1,
        kind: PlayerOperationKind::Teleport([2.5, 300.0, 0.5]),
    };
    let mut reads = TerrainReads::default();
    let mut requested = Vec::new();
    validate(
        &mut world,
        &mut reads,
        &mut requested,
        std::slice::from_ref(&operation),
        &[],
    )
    .unwrap();
    assert!(!reads.is_empty() && reads.is_current());
    world.edit(2, 300, 0, crate::world::STONE).unwrap();
    assert!(
        !reads.is_current(),
        "an air read did not fence a later obstruction"
    );
    let mut reads = TerrainReads::default();
    assert!(
        validate(
            &mut world,
            &mut reads,
            &mut requested,
            std::slice::from_ref(&operation),
            &[]
        )
        .is_err()
    );
    validate(
        &mut world,
        &mut TerrainReads::default(),
        &mut requested,
        std::slice::from_ref(&operation),
        &[(2, 300, 0, crate::world::AIR)],
    )
    .unwrap();
    world.edit(2, 300, 0, crate::world::AIR).unwrap();
    assert!(
        validate(
            &mut world,
            &mut TerrainReads::default(),
            &mut requested,
            &[operation],
            &[(2, 300, 0, crate::world::STONE)]
        )
        .is_err()
    );
    drop(world);
    std::fs::remove_dir_all(path).unwrap();
}
