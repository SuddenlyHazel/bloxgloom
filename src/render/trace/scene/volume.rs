//! Medium records appended to the loaded-space buffer; all offsets are words.
use super::{Chunk, Scene};
use std::sync::Arc;
mod index;

/// Preserve the original loaded-cell prefix. Near voxels override coarse
/// presentation intervals, which never become fine-world or sky certificates.
#[cfg(test)]
pub(crate) fn append(scene: &mut Scene, near: &[Arc<Chunk>], lod: &[Arc<Chunk>]) {
    append_limited(scene, near, lod, u64::MAX);
}

pub(crate) fn append_limited(
    scene: &mut Scene,
    near: &[Arc<Chunk>],
    lod: &[Arc<Chunk>],
    limit: u64,
) {
    let wet: Vec<_> = near
        .iter()
        .filter_map(|chunk| Some((chunk.key?, chunk.water.as_ref()?)))
        .collect();
    let coarse: Vec<_> = lod
        .iter()
        .filter_map(|chunk| chunk.coarse_water.as_ref())
        .collect();
    if wet.is_empty() && coarse.is_empty() {
        return;
    }
    let data = &mut scene.coverage;
    let start = data.len();
    scene.water_offset = start as u32;
    let near_directory = start + 4;
    let coarse_directory = near_directory + wet.len() * 4;
    data.resize(coarse_directory + coarse.len() * 8, 0);
    data[start..start + 4].copy_from_slice(&[
        wet.len() as u32,
        near_directory as u32,
        coarse.len() as u32,
        coarse_directory as u32,
    ]);
    // Bounded source keys have already established this cell domain.
    let minimum = [data[0] as i32, data[1] as i32, data[2] as i32];
    let dimensions = [data[4] as u64, data[5] as u64, data[6] as u64];
    let mut entries = Vec::with_capacity(wet.len());
    for (key, occupancy) in wet {
        let p = [key.x, key.y, key.z].map(i64::from);
        let relative = std::array::from_fn::<_, 3, _>(|i| p[i] - i64::from(minimum[i]));
        let cell = if relative
            .iter()
            .enumerate()
            .all(|(i, p)| *p >= 0 && (*p as u64) < dimensions[i])
        {
            relative[0] as u64
                + dimensions[0] * (relative[1] as u64 + dimensions[1] * relative[2] as u64)
        } else {
            u64::from(u32::MAX)
        };
        let offset = data.len();
        data.extend_from_slice(&occupancy.mask);
        entries.push([cell as u32, occupancy.class, offset as u32, 0]);
    }
    entries.sort_unstable_by_key(|e| e[0]);
    for (index, entry) in entries.iter().enumerate() {
        data[near_directory + index * 4..near_directory + index * 4 + 4].copy_from_slice(entry);
    }
    for (index, tile) in coarse.into_iter().enumerate() {
        let bounds = tile.key.bounds().expect("validated coarse volume key");
        let width = tile
            .key
            .sample_width()
            .expect("validated coarse sample width");
        let columns = data.len();
        data.resize(columns + tile.columns.len() * 4, 0);
        let entry = coarse_directory + index * 8;
        data[entry..entry + 8].copy_from_slice(&[
            bounds[0] as u32,
            bounds[1] as u32,
            width as u32,
            columns as u32,
            (bounds[2] - bounds[0]) as u32,
            tile.columns.len() as u32,
            0,
            0,
        ]);
        for (column_index, column) in tile.columns.iter().enumerate() {
            let coverage = data.len();
            for interval in &column.coverage {
                data.extend([interval.bottom as u32, interval.top as u32]);
            }
            let water = data.len();
            for interval in &column.water {
                data.extend([interval.bottom as u32, interval.top as u32]);
            }
            let entry = columns + column_index * 4;
            data[entry..entry + 4].copy_from_slice(&[
                coverage as u32,
                column.coverage.len() as u32,
                water as u32,
                column.water.len() as u32,
            ]);
        }
    }
    let coarse_count = data[start + 2] as usize;
    index::append(data, coarse_directory, coarse_count, limit);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_near_water_preserves_loaded_prefix_and_exact_voxel_bit_order() {
        let key = crate::world::ChunkKey { x: -1, y: 2, z: -3 };
        let mut source = crate::world::Chunk::from_blocks(
            key,
            0,
            vec![crate::world::AIR; crate::world::CHUNK_VOLUME],
        );
        let local = [3, 7, 11];
        let index = crate::world::Chunk::index(local).unwrap();
        source.blocks.set(index, crate::world::WATER);
        let chunk = Arc::new(Chunk {
            key: Some(key),
            triangles: Vec::new(),
            water: super::super::water::Occupancy::from_chunk(&source, crate::content::catalog()),
            coarse_water: None,
        });
        let mut scene = Scene::build([chunk.clone()]);
        let prefix = scene.coverage.clone();
        append(&mut scene, &[chunk], &[]);
        assert_eq!(scene.coverage[..prefix.len()], prefix);
        let start = scene.water_offset as usize;
        assert_eq!(scene.coverage[start], 1);
        let entry = scene.coverage[start + 1] as usize;
        assert_eq!(scene.coverage[entry..entry + 2], [0, 2]);
        let mask = scene.coverage[entry + 2] as usize;
        assert_eq!(scene.coverage[mask + index / 32], 1 << (index % 32));
        assert_eq!(index, 3 + 16 * (11 + 16 * 7));
        assert!(scene.fits(scene.coverage.len() as u64 * 4));
        assert!(!scene.fits(scene.coverage.len() as u64 * 4 - 1));
    }
}
