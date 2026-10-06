//! Binned surface-area split selection for triangle BVHs.
use super::Triangle;
use glam::Vec3;

const BINS: usize = 16;
#[derive(Clone, Copy)]
struct Bounds {
    low: Vec3,
    high: Vec3,
    count: usize,
}
impl Bounds {
    const EMPTY: Self = Self {
        low: Vec3::splat(f32::INFINITY),
        high: Vec3::splat(f32::NEG_INFINITY),
        count: 0,
    };
    fn add(&mut self, other: Self) {
        self.low = self.low.min(other.low);
        self.high = self.high.max(other.high);
        self.count += other.count;
    }
    fn area(self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        // Match the production .12m wind expansion on both sides.
        let d = self.high - self.low + Vec3::splat(0.24);
        2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
    }
}
fn bounds(t: &Triangle) -> Bounds {
    let a = Vec3::from_slice(&t.a[..3]);
    let b = Vec3::from_slice(&t.b[..3]);
    let c = Vec3::from_slice(&t.c[..3]);
    Bounds {
        low: a.min(b).min(c),
        high: a.max(b).max(c),
        count: 1,
    }
}
fn centroid(t: &Triangle) -> Vec3 {
    (Vec3::from_slice(&t.a[..3]) + Vec3::from_slice(&t.b[..3]) + Vec3::from_slice(&t.c[..3])) / 3.0
}

pub(super) fn split(triangles: &mut [Triangle]) -> usize {
    let mut low = Vec3::splat(f32::INFINITY);
    let mut high = Vec3::splat(f32::NEG_INFINITY);
    for t in triangles.iter() {
        let c = centroid(t);
        low = low.min(c);
        high = high.max(c);
    }
    let extent = high - low;
    let mut all_bins = [[Bounds::EMPTY; BINS]; 3];
    let bin = |c: f32, axis: usize| (((c - low[axis]) / extent[axis]) * BINS as f32) as usize;
    for t in triangles.iter() {
        let c = centroid(t);
        let bounds = bounds(t);
        for (axis, bins) in all_bins.iter_mut().enumerate() {
            if extent[axis] > 0.00001 {
                bins[bin(c[axis], axis).min(BINS - 1)].add(bounds);
            }
        }
    }
    let mut best = None;
    let mut best_cost = f32::INFINITY;
    for (axis, bins) in all_bins.iter().enumerate() {
        if extent[axis] <= 0.00001 {
            continue;
        }
        let mut right = [Bounds::EMPTY; BINS];
        let mut aggregate = Bounds::EMPTY;
        for (i, bin) in bins.iter().enumerate().rev() {
            aggregate.add(*bin);
            right[i] = aggregate;
        }
        let mut left = Bounds::EMPTY;
        for (i, (bin, right)) in bins.iter().zip(right.iter().skip(1)).enumerate() {
            left.add(*bin);
            if left.count == 0 || right.count == 0 {
                continue;
            }
            let cost = left.area() * left.count as f32 + right.area() * right.count as f32;
            if cost < best_cost {
                best_cost = cost;
                best = Some((axis, i));
            }
        }
    }
    if let Some((axis, boundary)) = best {
        let mut split = 0;
        for i in 0..triangles.len() {
            if bin(centroid(&triangles[i])[axis], axis).min(BINS - 1) <= boundary {
                triangles.swap(i, split);
                split += 1;
            }
        }
        if split > 0 && split < triangles.len() {
            return split;
        }
    }
    // Coincident centroids still terminate with nonempty, balanced children.
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    let split = triangles.len() / 2;
    triangles.select_nth_unstable_by(split, |a, b| {
        centroid(a)[axis].total_cmp(&centroid(b)[axis])
    });
    split
}
