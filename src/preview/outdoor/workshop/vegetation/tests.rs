use super::*;

#[test]
fn crowns_are_bounded_connected_and_have_distinct_silhouettes() {
    let mut silhouettes = std::collections::HashSet::new();
    for (index, &(_, _, _, r)) in TREES.iter().enumerate() {
        let mut cells = std::collections::HashSet::new();
        let mut profile = Vec::new();
        for dy in -6..=6 {
            let mut layer = 0;
            for dz in -8..=8 {
                for dx in -8..=8 {
                    if crown(dx, dy, dz, r, index) {
                        assert!(dx.abs() <= r + 2 && dz.abs() <= r + 2 && dy.abs() <= 5);
                        cells.insert((dx, dy, dz));
                        layer += 1;
                    }
                }
            }
            profile.push(layer);
        }
        assert!(
            (150..500).contains(&cells.len()),
            "bounded canopy volume: {}",
            cells.len()
        );
        silhouettes.insert(profile);
        let mut connected = std::collections::HashSet::from([(0, 0, 0)]);
        let mut pending = vec![(0, 0, 0)];
        while let Some((x, y, z)) = pending.pop() {
            for next in [
                (x - 1, y, z),
                (x + 1, y, z),
                (x, y - 1, z),
                (x, y + 1, z),
                (x, y, z - 1),
                (x, y, z + 1),
            ] {
                if cells.contains(&next) && connected.insert(next) {
                    pending.push(next);
                }
            }
        }
        assert_eq!(connected, cells, "no detached leaf islands");
    }
    assert!(
        silhouettes.len() >= 5,
        "height/radius profiles must not be clones"
    );
}
