//! Derived 3D buckets for local drop queries.
use std::collections::{HashMap, HashSet};

pub(super) const DEFAULT_BUCKET_SIZE: i32 = 16;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Bucket {
    x: i32,
    y: i32,
    z: i32,
}

/// A drop belongs to exactly one bucket. Hash iteration is never observable:
/// query results are sorted by ID before the caller applies its exact filter.
#[derive(Clone)]
pub(super) struct DropSpatialIndex {
    bucket_size: f32,
    buckets: HashMap<Bucket, HashSet<u64>>,
    bucket_by_id: HashMap<u64, Bucket>,
}

impl DropSpatialIndex {
    pub(super) fn new() -> Self {
        Self::with_bucket_size(DEFAULT_BUCKET_SIZE)
    }

    pub(super) fn with_bucket_size(bucket_size: i32) -> Self {
        assert!(bucket_size > 0);
        Self {
            bucket_size: bucket_size as f32,
            buckets: HashMap::new(),
            bucket_by_id: HashMap::new(),
        }
    }

    pub(super) fn insert(&mut self, id: u64, position: [f32; 3]) {
        self.remove(id);
        let bucket = self.bucket_for(position);
        self.buckets.entry(bucket).or_default().insert(id);
        self.bucket_by_id.insert(id, bucket);
    }

    pub(super) fn remove(&mut self, id: u64) {
        let Some(bucket) = self.bucket_by_id.remove(&id) else {
            return;
        };
        if let Some(ids) = self.buckets.get_mut(&bucket) {
            ids.remove(&id);
            if ids.is_empty() {
                self.buckets.remove(&bucket);
            }
        }
    }

    /// Updates membership only when the point crosses a bucket boundary.
    pub(super) fn move_to(&mut self, id: u64, position: [f32; 3]) {
        let bucket = self.bucket_for(position);
        if self.bucket_by_id.get(&id) == Some(&bucket) {
            return;
        }
        self.remove(id);
        self.buckets.entry(bucket).or_default().insert(id);
        self.bucket_by_id.insert(id, bucket);
    }

    /// Returns broad-phase candidates in deterministic ID order. Bounds are
    /// inclusive; callers apply the existing strict or inclusive exact test.
    pub(super) fn query_aabb(&self, min: [f32; 3], max: [f32; 3]) -> Vec<u64> {
        if min
            .iter()
            .chain(max.iter())
            .any(|coordinate| !coordinate.is_finite())
            || (0..3).any(|axis| min[axis] > max[axis])
        {
            return Vec::new();
        }

        let min_bucket = self.bucket_for(min);
        let max_bucket = self.bucket_for(max);
        let mut candidates = Vec::new();
        let mut x = min_bucket.x;
        loop {
            let mut y = min_bucket.y;
            loop {
                let mut z = min_bucket.z;
                loop {
                    if let Some(ids) = self.buckets.get(&Bucket { x, y, z }) {
                        candidates.extend(ids.iter().copied());
                    }
                    if z == max_bucket.z {
                        break;
                    }
                    z += 1;
                }
                if y == max_bucket.y {
                    break;
                }
                y += 1;
            }
            if x == max_bucket.x {
                break;
            }
            x += 1;
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
    }

    pub(super) fn bucket_size(&self) -> i32 {
        self.bucket_size as i32
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.bucket_by_id.len()
    }

    fn bucket_for(&self, position: [f32; 3]) -> Bucket {
        let coordinate = |value: f32| (value / self.bucket_size).floor() as i32;
        Bucket {
            x: coordinate(position[0]),
            y: coordinate(position[1]),
            z: coordinate(position[2]),
        }
    }

    #[cfg(test)]
    pub(super) fn contains(&self, id: u64) -> bool {
        self.bucket_by_id.contains_key(&id)
    }
}
