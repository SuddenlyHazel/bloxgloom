//! Bounded distant target pages, built off the window thread.
use super::{Chunk, Scene, Triangle};
use std::sync::Arc;

pub(crate) const MAX_PAGES: usize = 4;
const HEADER_BYTES: u64 = 16;

/// Reserve the worst-case binary tree (two nodes per triangle) before copying.
/// Overflow selects the complete raster fallback, never a partial ray scene.
pub(crate) fn build(
    chunks: impl IntoIterator<Item = Arc<Chunk>>,
    storage_limit: u64,
) -> Option<Vec<Scene>> {
    let chunks: Vec<_> = chunks.into_iter().collect();
    let count = chunks
        .iter()
        .try_fold(0usize, |n, c| n.checked_add(c.triangles.len()))?;
    if count == 0 {
        return Some(Vec::new());
    }
    let bytes_per_triangle =
        std::mem::size_of::<Triangle>() as u64 + 2 * std::mem::size_of::<super::Node>() as u64;
    let capacity = usize::try_from(storage_limit.checked_sub(HEADER_BYTES)? / bytes_per_triangle)
        .ok()?
        .min(0x0fff_ffff);
    if capacity == 0 || count > capacity.checked_mul(MAX_PAGES)? {
        return None;
    }
    let mut pages = Vec::new();
    let mut triangles = Vec::with_capacity(count.min(capacity));
    let finish = |triangles: Vec<Triangle>| {
        let mut scene = Scene {
            triangles,
            ..Default::default()
        };
        let count = scene.triangles.len();
        scene.partition(0, count, super::super::optimizations::tight_bounds());
        scene
    };
    for chunk in chunks {
        let mut source = chunk.triangles.as_slice();
        while !source.is_empty() {
            let take = source.len().min(capacity - triangles.len());
            triangles.extend_from_slice(&source[..take]);
            source = &source[take..];
            if triangles.len() == capacity {
                pages.push(finish(std::mem::take(&mut triangles)));
                triangles = Vec::with_capacity((count - pages.len() * capacity).min(capacity));
            }
        }
    }
    if !triangles.is_empty() {
        pages.push(finish(triangles));
    }
    Some(pages)
}

/// Header and records share one storage binding. All offsets are u32 words.
pub(crate) fn packed_words(scene: &Scene) -> Vec<u32> {
    let node_words: &[u32] = bytemuck::cast_slice(&scene.nodes);
    let triangle_words: &[u32] = bytemuck::cast_slice(&scene.triangles);
    let mut words = Vec::with_capacity(4 + node_words.len() + triangle_words.len());
    words.extend([
        scene.nodes.len() as u32,
        scene.triangles.len() as u32,
        4,
        (4 + node_words.len()) as u32,
    ]);
    words.extend_from_slice(node_words);
    words.extend_from_slice(triangle_words);
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn pages_preserve_all_surface_bits_and_never_certify_loaded_air() {
        let chunks = [Arc::new(Chunk {
            water: None,
            coarse_water: None,

            key: Some(crate::world::ChunkKey { x: -1, y: 0, z: 1 }),
            triangles: (0..4)
                .map(|n| {
                    let mut triangle = Triangle::zeroed().with_surface([1.0; 4], 21);
                    triangle.a[0] = n as f32;
                    triangle.b[0] = n as f32 + 1.0;
                    triangle.c[1] = 1.0;
                    triangle
                })
                .collect(),
        })];
        // Deliberately tiny pages force cross-page preservation of identical
        // topology/material records and exact encoded-ID capacity.
        let pages = build(chunks, 208).unwrap();
        assert_eq!(pages.len(), 4);
        for (n, page) in pages.iter().enumerate() {
            assert!(page.coverage.is_empty());
            let words = packed_words(page);
            assert!(words.len() * 4 <= 208);
            assert_eq!(words[..4], [1, 1, 4, 16]);
            let start = words[3] as usize;
            assert_eq!(words[start + 18], u32::MAX);
            assert_eq!(words[start + 19], 21);
            assert_eq!(f32::from_bits(words[start]), n as f32);
        }
    }

    #[test]
    fn a_scene_exceeding_all_pages_is_rejected_as_a_whole() {
        let chunk = Arc::new(Chunk {
            water: None,
            coarse_water: None,

            key: None,
            triangles: vec![Triangle::zeroed(); 5],
        });
        assert!(build([chunk], 208).is_none());
        assert!(build([], 0).unwrap().is_empty());
    }
}
