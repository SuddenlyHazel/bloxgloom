//! Bounded, deterministic chunk-interest traversal for replication and prefetch.
//!
//! Distance order is a presentation priority only. The authoritative chunk
//! comes from the resident world cache or the asynchronous chunk loader.

use crate::world::ChunkKey;
use std::collections::HashSet;

#[cfg(test)]
#[path = "interest/tests.rs"]
mod tests;

/// Returns at most `limit` unsent keys, nearest first. The vertical interest
/// range is the current three-chunk band, independent of horizontal radius.
pub(super) fn nearest_unsent(
    center: ChunkKey,
    radius: u8,
    sent: &HashSet<ChunkKey>,
    limit: usize,
) -> Vec<ChunkKey> {
    if limit == 0 {
        return Vec::new();
    }
    let radius = i32::from(radius);
    let mut keys = Vec::with_capacity(limit.min((2 * radius as usize + 1).pow(2) * 3));
    for distance in 0..=(radius * 2 + 1) {
        for y in -1i32..=1 {
            for z in -radius..=radius {
                // Solve the Manhattan shell for x instead of rescanning the
                // entire interest volume at every distance. The two signed
                // candidates preserve the original nearest-first tie order.
                let x_magnitude = distance - y.abs() - z.abs();
                if !(0..=radius).contains(&x_magnitude) {
                    continue;
                }
                let x_count = if x_magnitude == 0 { 1 } else { 2 };
                for x in [-x_magnitude, x_magnitude].into_iter().take(x_count) {
                    let (Some(x), Some(y), Some(z)) = (
                        center.x.checked_add(x),
                        center.y.checked_add(y),
                        center.z.checked_add(z),
                    ) else {
                        continue;
                    };
                    let key = ChunkKey { x, y, z };
                    if !sent.contains(&key) {
                        keys.push(key);
                        if keys.len() == limit {
                            return keys;
                        }
                    }
                }
            }
        }
    }
    keys
}
