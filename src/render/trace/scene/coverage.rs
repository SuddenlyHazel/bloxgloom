//! Bounded loaded-space certificates. Missing cells always remain unknown.
use crate::world::{CHUNK_SIZE, ChunkKey, MAX_GENERATED_HEIGHT};
use std::collections::BTreeSet;

const MAX_CELLS: u64 = 8 * 1024 * 1024;
/// Header: minimum XYZ (signed bit patterns), exterior Y; dimensions XYZ, cells.
/// The following words contain one loaded bit per 3D chunk, X fastest.
pub(super) fn build(keys: impl IntoIterator<Item = ChunkKey>) -> Vec<u32> {
    let keys: BTreeSet<_> = keys.into_iter().collect();
    // Runtime arrays require one element and 16-byte struct alignment even
    // when cells=0 explicitly disables lookup.
    let unknown = || vec![0; 12];
    if keys.is_empty() {
        return unknown();
    }
    let min = [
        keys.iter().map(|k| k.x).min().unwrap(),
        keys.iter().map(|k| k.y).min().unwrap(),
        keys.iter().map(|k| k.z).min().unwrap(),
    ];
    let max = [
        keys.iter().map(|k| k.x).max().unwrap(),
        keys.iter().map(|k| k.y).max().unwrap(),
        keys.iter().map(|k| k.z).max().unwrap(),
    ];
    let dimensions =
        std::array::from_fn::<_, 3, _>(|i| (i64::from(max[i]) - i64::from(min[i]) + 1) as u64);
    let cells = dimensions
        .into_iter()
        .try_fold(1u64, |n, d| n.checked_mul(d));
    let Some(cells) = cells.filter(|n| *n <= MAX_CELLS) else {
        return unknown();
    };
    let upper = (i64::from(max[1]) + 1) * CHUNK_SIZE as i64;
    let upper = upper.max(i64::from(MAX_GENERATED_HEIGHT) + 1);
    if upper > i64::from(i32::MAX) {
        return unknown();
    }
    let words = (8 + cells.div_ceil(32) as usize).next_multiple_of(4);
    let mut data = vec![0; words];
    data[..8].copy_from_slice(&[
        min[0] as u32,
        min[1] as u32,
        min[2] as u32,
        upper as u32,
        dimensions[0] as u32,
        dimensions[1] as u32,
        dimensions[2] as u32,
        cells as u32,
    ]);
    for key in keys {
        let x = (i64::from(key.x) - i64::from(min[0])) as u64;
        let y = (i64::from(key.y) - i64::from(min[1])) as u64;
        let z = (i64::from(key.z) - i64::from(min[2])) as u64;
        let index = x + dimensions[0] * (y + dimensions[1] * z);
        data[8 + (index / 32) as usize] |= 1u32 << (index % 32);
    }
    data
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_loaded_cells_negative_coordinates_and_holes_are_distinct() {
        let data = build([
            ChunkKey { x: -2, y: 0, z: -1 },
            ChunkKey { x: 0, y: 0, z: -1 },
        ]);
        assert_eq!(data[..8], [-2i32 as u32, 0, -1i32 as u32, 113, 3, 1, 1, 3]);
        assert_eq!(data[8], 0b101);
        assert_eq!(build([]), vec![0; 12]);
    }
    #[test]
    fn empty_mesh_keys_survive_scene_build_and_coverage_has_its_own_storage_quota() {
        use super::super::{Chunk, Scene};
        use std::sync::Arc;
        let key = ChunkKey { x: -1, y: 7, z: 2 };
        let scene = Scene::build([Arc::new(Chunk {
            water: None,
            coarse_water: None,

            key: Some(key),
            triangles: vec![],
        })]);
        assert!(scene.nodes.is_empty());
        assert_eq!(scene.coverage[3], 128);
        assert_eq!(scene.coverage[8], 1);
        assert!(scene.fits(48));
        assert!(!scene.fits(47));
    }
    #[test]
    fn pathological_sparse_ranges_are_unknown_without_large_allocations() {
        assert_eq!(
            build([
                ChunkKey {
                    x: i32::MIN,
                    y: 0,
                    z: 0
                },
                ChunkKey {
                    x: i32::MAX,
                    y: 0,
                    z: 0
                }
            ]),
            vec![0; 12]
        );
        assert_eq!(
            build([ChunkKey {
                x: 0,
                y: i32::MAX,
                z: 0
            }]),
            vec![0; 12]
        );
    }
}
