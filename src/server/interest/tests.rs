use super::*;

#[test]
fn visits_every_interest_key_once_in_distance_order() {
    let center = ChunkKey { x: -11, y: 5, z: 9 };
    let keys = nearest_unsent(center, 3, &HashSet::new(), usize::MAX);
    assert_eq!(keys.len(), 3 * 7 * 7);
    assert_eq!(keys[0], center);
    assert_eq!(
        keys.iter().copied().collect::<HashSet<_>>().len(),
        keys.len()
    );
    assert!(
        keys.windows(2)
            .all(|pair| distance(center, pair[0]) <= distance(center, pair[1]))
    );
    assert!(keys.iter().all(|key| {
        (key.x - center.x).abs() <= 3
            && (key.y - center.y).abs() <= 1
            && (key.z - center.z).abs() <= 3
    }));
}

#[test]
fn sent_keys_are_skipped_without_expanding_the_budget() {
    let center = ChunkKey { x: 0, y: 0, z: 0 };
    let mut sent = HashSet::new();
    sent.insert(center);
    let keys = nearest_unsent(center, 1, &sent, 4);
    assert_eq!(keys.len(), 4);
    assert!(!keys.contains(&center));
    assert!(keys.iter().all(|key| distance(center, *key) == 1));
    assert!(nearest_unsent(center, 1, &sent, 0).is_empty());
}

#[test]
fn boundaries_skip_unrepresentable_chunk_keys() {
    let center = ChunkKey {
        x: i32::MAX,
        y: i32::MIN,
        z: i32::MAX,
    };
    let keys = nearest_unsent(center, 1, &HashSet::new(), usize::MAX);
    assert_eq!(keys.len(), 2 * 2 * 2);
    assert!(
        keys.iter()
            .all(|key| key.x <= center.x && key.y >= center.y && key.z <= center.z)
    );
}

#[test]
fn optimized_shell_walk_preserves_nearest_first_tie_order() {
    let center = ChunkKey { x: -4, y: 2, z: 7 };
    for radius in 1i32..=6 {
        let mut expected = Vec::new();
        for distance in 0..=(2 * radius + 1) {
            for y in -1i32..=1 {
                for z in -radius..=radius {
                    for x in -radius..=radius {
                        if x.abs() + y.abs() + z.abs() == distance {
                            expected.push(ChunkKey {
                                x: center.x + x,
                                y: center.y + y,
                                z: center.z + z,
                            });
                        }
                    }
                }
            }
        }
        assert_eq!(
            nearest_unsent(center, radius as u8, &HashSet::new(), usize::MAX),
            expected
        );
    }
}

fn distance(center: ChunkKey, key: ChunkKey) -> i64 {
    i64::from(key.x).abs_diff(i64::from(center.x)) as i64
        + i64::from(key.y).abs_diff(i64::from(center.y)) as i64
        + i64::from(key.z).abs_diff(i64::from(center.z)) as i64
}
