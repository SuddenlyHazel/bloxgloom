use super::*;
use crate::world::{self, AIR, WATER};

#[test]
fn seeded_features_are_deterministic_flat_and_keep_spawn_dry() {
    for seed in [0xB10C_6100, 17, 99] {
        let mut sampler = Sampler::new(seed);
        let mut reverse = Sampler::new(seed);
        // Warm caches in the opposite spatial order before comparing results.
        let coordinates = (-512..=512).step_by(8).collect::<Vec<_>>();
        for &z in coordinates.iter().rev() {
            for &x in coordinates.iter().rev() {
                reverse.column(x, z);
            }
        }
        let mut counts = [0usize; 3];
        for z in (-512..=512).step_by(8) {
            for x in (-512..=512).step_by(8) {
                let column = sampler.column(x, z);
                let same = reverse.column(x, z);
                assert_eq!(
                    (column.height, column.water_level, column.water_kind),
                    (same.height, same.water_level, same.water_kind)
                );
                if let Some(level) = column.water_level {
                    assert!(column.height < level);
                    counts[column.water_kind.unwrap() as usize] += 1;
                    if x * x + z * z >= 40 * 40 {
                        assert!(
                            level - column.height
                                <= match column.water_kind.unwrap() {
                                    Kind::River => 5,
                                    Kind::Lake => 7,
                                    Kind::Pond => 3,
                                }
                        );
                    }
                    if column.water_kind == Some(Kind::River) {
                        assert_eq!(level, 16);
                    }
                    assert_eq!(super::super::generated_block(x, level, z, seed), WATER);
                    assert_eq!(super::super::generated_block(x, level + 1, z, seed), AIR);
                    assert_ne!(
                        super::super::generated_block(x, column.height, z, seed),
                        AIR
                    );
                }
            }
        }
        assert!(
            counts.into_iter().all(|count| count > 0),
            "missing feature for seed {seed}: {counts:?}"
        );
        for z in -16..=16 {
            for x in -16..=16 {
                assert!(sampler.column(x, z).water_level.is_none());
            }
        }
    }
}

#[test]
fn generated_water_matches_exact_columns_across_negative_chunk_seams() {
    let seed = 0xB10C_6100;
    let mut sampler = Sampler::new(seed);
    let location = (-384..=0)
        .step_by(16)
        .find_map(|x| {
            (-128..=128)
                .step_by(16)
                .find_map(|z| sampler.column(x, z).water_level.map(|level| (x, z, level)))
        })
        .unwrap();
    let (key, _) = world::world_to_chunk(location.0 as i32, location.2 as i32, location.1 as i32);
    for dx in [0, 1] {
        for dz in [0, 1] {
            let key = world::ChunkKey {
                x: key.x + dx,
                y: key.y,
                z: key.z + dz,
            };
            let chunk = world::generate_chunk(key, seed);
            for y in 0..world::CHUNK_SIZE {
                for z in 0..world::CHUNK_SIZE {
                    for x in 0..world::CHUNK_SIZE {
                        let wx = i64::from(key.x) * 16 + x as i64;
                        let wy = i64::from(key.y) * 16 + y as i64;
                        let wz = i64::from(key.z) * 16 + z as i64;
                        assert_eq!(
                            chunk.block([x, y, z]),
                            Some(super::super::generated_block(wx, wy, wz, seed)),
                            "{wx} {wy} {wz}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn water_survives_distant_lod_without_exceeding_transport_budgets() {
    for x in [0, 2, -2] {
        let tile = crate::world::lod::builtin_lod_tile(
            crate::lod::TileKey { level: 4, x, z: 0 },
            1,
            7,
            crate::content::catalog(),
        )
        .unwrap();
        assert!(
            tile.columns
                .iter()
                .flat_map(|c| &c.spans)
                .any(|span| span.state == WATER)
        );
    }
}
