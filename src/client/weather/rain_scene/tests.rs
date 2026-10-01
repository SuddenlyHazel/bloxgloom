use super::*;

#[test]
fn sample_tracks_canopy_ground_edits_and_unknown_chunks() {
    let catalog = crate::content::Catalog::builtins();
    let eye = Vec3::new(0.5, 2.0, 0.5);
    let mut sample_world = |x, y, z| {
        Some(if x == 0 && z == 0 && y == 5 {
            world::LEAVES
        } else if y == 0 {
            world::STONE
        } else {
            world::AIR
        })
    };
    let scene = sample(eye, 12, &catalog, &mut sample_world);
    let tops: Vec<_> = scene
        .tiles
        .iter()
        .filter(|t| t.normal == [0.0; 2])
        .collect();
    assert_eq!(tops.len(), 256);
    assert_eq!(
        tops.iter()
            .filter(|t| t.material == RainMaterial::Leaf)
            .count(),
        1
    );
    assert!(
        !tops.iter().any(|t| t.centre == [0.5, 1.0, 0.5]),
        "ground below canopy must not receive rain"
    );
    let cleared = sample(eye, 12, &catalog, |_, y, _| {
        Some(if y == 0 { world::STONE } else { world::AIR })
    });
    assert!(
        cleared
            .tiles
            .iter()
            .all(|t| t.material == RainMaterial::Concrete)
    );
    let unknown = sample(eye, 12, &catalog, |_, _, _| None);
    assert!(unknown.tiles.is_empty());
    let missing_above_ground = sample(eye, 12, &catalog, |_, y, _| {
        if y == 8 {
            None
        } else {
            Some(if y == 0 { world::STONE } else { world::AIR })
        }
    });
    assert!(missing_above_ground.tiles.is_empty());
}

#[test]
fn builtin_acoustic_mapping_preserves_wood_states_and_soft_ground() {
    let catalog = crate::content::Catalog::builtins();
    for id in [world::WOOD, world::WOOD_X, world::WOOD_Z] {
        assert_eq!(material(&catalog, id), Some(RainMaterial::Wood));
    }
    for id in [world::GRASS, world::DIRT, world::MOSS, world::SNOW] {
        assert_eq!(material(&catalog, id), Some(RainMaterial::Dirt));
    }
    assert_eq!(material(&catalog, world::LEAVES), Some(RainMaterial::Leaf));
    assert_eq!(material(&catalog, world::AIR), None);
}
