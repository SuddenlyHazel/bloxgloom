//! Conservative sky-scan ceilings from saved overrides, including unloaded roofs.
//! The directory is read only at world open. Live edits update this in memory.
use super::{CHUNK_SIZE, ChunkKey, MAX_GENERATED_HEIGHT};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

#[derive(Default)]
pub(super) struct Ceiling {
    columns: BTreeMap<(i32, i32), BTreeSet<i32>>,
}

impl Ceiling {
    pub(super) fn open(root: &Path, storage: &crate::storage::Storage) -> io::Result<Self> {
        let mut result = Self::default();
        for entry in std::fs::read_dir(root)? {
            let name = entry?.file_name();
            let Some(stem) = name.to_str().and_then(|name| name.strip_suffix(".bged")) else {
                continue;
            };
            let parts: Vec<_> = stem.split('_').collect();
            if let [x, y, z] = parts.as_slice()
                && let (Ok(x), Ok(y), Ok(z)) = (x.parse(), y.parse(), z.parse())
            {
                let key = ChunkKey { x, y, z };
                if y >= MAX_GENERATED_HEIGHT.div_euclid(CHUNK_SIZE as i32) {
                    result.update(key, !storage.load(key)?.blocks.is_empty());
                }
            }
        }
        Ok(result)
    }

    pub(super) fn update(&mut self, key: ChunkKey, edited: bool) {
        if key.y < MAX_GENERATED_HEIGHT.div_euclid(CHUNK_SIZE as i32) {
            return;
        }
        let column = (key.x, key.z);
        if edited {
            self.columns.entry(column).or_default().insert(key.y);
        } else if let Some(heights) = self.columns.get_mut(&column) {
            heights.remove(&key.y);
            if heights.is_empty() {
                self.columns.remove(&column);
            }
        }
    }

    pub(super) fn top(&self, x: i32, z: i32) -> i32 {
        let column = (
            x.div_euclid(CHUNK_SIZE as i32),
            z.div_euclid(CHUNK_SIZE as i32),
        );
        self.columns
            .get(&column)
            .and_then(BTreeSet::last)
            .map_or(MAX_GENERATED_HEIGHT, |y| {
                (i64::from(*y) * CHUNK_SIZE as i64 + CHUNK_SIZE as i64 - 1)
                    .clamp(i64::from(MAX_GENERATED_HEIGHT), i64::from(i32::MAX))
                    as i32
            })
    }
}

impl super::World {
    /// Custom contributors have no upper-height contract, so an empty finite
    /// scan cannot prove that their column reaches the sky.
    pub(crate) fn sky_scan_top(&self, x: i32, z: i32) -> Option<i32> {
        self.generator
            .is_builtin()
            .then(|| self.sky_ceiling.top(x, z))
    }
}
