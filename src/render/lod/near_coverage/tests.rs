use super::*;
#[test]
fn ready_coverage_initializes_empty_and_tracks_additions_removals_and_hash_collisions() {
    let mut cache = NearCoverage::default();
    assert!(
        cache
            .update(std::iter::empty())
            .unwrap()
            .iter()
            .all(|v| v[3] == 0)
    );
    assert!(cache.update(std::iter::empty()).is_none());
    let a = ChunkKey { x: -1, y: 40, z: 0 };
    let b = ChunkKey {
        x: a.x + SLOTS as i32,
        ..a
    };
    assert_eq!(hash(a), hash(b));
    let slots = cache.update([a, b].into_iter()).unwrap();
    for k in [a, b] {
        let mut index = hash(k);
        loop {
            assert_ne!(slots[index][3], 0);
            if slots[index][..3] == [k.x, k.y, k.z] {
                break;
            }
            index = (index + 1) & (SLOTS - 1);
        }
    }
    assert!(cache.update([b, a].into_iter()).is_none());
    let slots = cache.update([b].into_iter()).unwrap();
    assert!(!slots.contains(&[a.x, a.y, a.z, 1]));
    assert!(slots.contains(&[b.x, b.y, b.z, 1]));
    assert!(
        cache
            .update(std::iter::empty())
            .unwrap()
            .iter()
            .all(|v| v[3] == 0)
    );
}
