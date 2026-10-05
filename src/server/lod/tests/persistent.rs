use super::*;
fn request(service: &Service, world: &World, key: TileKey, revision: u64) -> worker::Completion {
    service
        .jobs
        .as_ref()
        .unwrap()
        .send(worker::Job {
            key,
            revision,
            overlays: world.lod_overlays(key.bounds().unwrap()).unwrap(),
            resident: world.lod_resident(key.bounds().unwrap()).unwrap(),
            children: None,
            requested_at: Instant::now(),
            cancelled: Arc::new(AtomicBool::new(false)),
        })
        .unwrap();
    service
        .results
        .recv_timeout(Duration::from_secs(20))
        .unwrap()
}
fn key() -> TileKey {
    TileKey {
        level: 0,
        x: 0,
        z: 0,
    }
}
#[test]
fn restart_reuses_unchanged_tile_and_rebases_its_publication_revision() {
    let root = temporary();
    let mut world = World::with_capacity(7, root.clone(), 4).unwrap();
    world.edit(0, 100, 0, crate::world::GLOWSTONE).unwrap();
    let service = Service::new(&world, root.clone()).unwrap();
    let cold = request(&service, &world, key(), 25);
    assert!(!cold.cache_hit);
    let mut expected = cold.tile.unwrap();
    drop(service);
    // An unrelated durable edit must not discard the explored landscape.
    world
        .edit(1000, 100, 1000, crate::world::GLOWSTONE)
        .unwrap();
    drop(world);
    let world = World::with_capacity(7, root.clone(), 4).unwrap();
    let service = Service::new(&world, root.clone()).unwrap();
    let warm = request(&service, &world, key(), 1);
    assert!(warm.cache_hit);
    expected.revision = 1;
    assert_eq!(warm.tile, Some(expected));
    eprintln!(
        "LOD restart: cold={:.3}ms warm={:.3}ms",
        cold.elapsed.as_secs_f64() * 1000.0,
        warm.elapsed.as_secs_f64() * 1000.0
    );
    drop(service);
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn committed_edit_before_invalidation_or_checkpoint_never_reuses_stale_disk_data() {
    let root = temporary();
    let mut world = World::with_capacity(7, root.clone(), 4).unwrap();
    let service = Service::new(&world, root.clone()).unwrap();
    assert!(request(&service, &world, key(), 1).tile.is_some());
    // Commit to memory while leaving both the old cache and checkpoint intact.
    world.get_block(0, 100, 0).unwrap();
    let edit = world
        .prepare_edit(0, 100, 0, crate::world::GLOWSTONE)
        .unwrap();
    let snapshot = edit.after_snapshot.clone();
    let chunk = edit.key;
    world.apply_prepared_edit(edit).unwrap();
    let changed = request(&service, &world, key(), 2);
    assert!(!changed.cache_hit);
    assert!(
        changed.tile.as_ref().unwrap().columns[0]
            .spans
            .iter()
            .any(|s| s.bottom <= 100 && s.top > 100 && s.state == crate::world::GLOWSTONE)
    );
    drop(service);
    // Checkpoint/recovery installs the same canonical after-value. Session
    // revision resets, but the source fingerprint remains reusable.
    world
        .storage_handle()
        .replace_snapshot(chunk, Some(&snapshot))
        .unwrap();
    drop(world);
    let world = World::with_capacity(7, root.clone(), 4).unwrap();
    let service = Service::new(&world, root.clone()).unwrap();
    assert!(request(&service, &world, key(), 1).cache_hit);
    drop(service);
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn different_world_identity_and_corrupt_cache_rebuild_from_authoritative_sources() {
    let root = temporary();
    let other = temporary();
    let world = World::with_capacity(7, root.clone(), 4).unwrap();
    let service = Service::new(&world, root.clone()).unwrap();
    assert!(request(&service, &world, key(), 1).tile.is_some());
    drop(service);
    drop(world);
    let world = World::with_capacity(8, other.clone(), 4).unwrap();
    std::fs::create_dir_all(other.join("lod-cache")).unwrap();
    let path = super::super::disk::path(&other.join("lod-cache"), key());
    std::fs::copy(
        super::super::disk::path(&root.join("lod-cache"), key()),
        &path,
    )
    .unwrap();
    let service = Service::new(&world, other.clone()).unwrap();
    let rebuilt = request(&service, &world, key(), 1);
    assert!(!rebuilt.cache_hit);
    assert!(rebuilt.tile.is_some());
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[80] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    let repaired = request(&service, &world, key(), 1);
    assert!(!repaired.cache_hit);
    assert_eq!(repaired.tile, rebuilt.tile);
    drop(service);
    drop(world);
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(other).unwrap();
}
