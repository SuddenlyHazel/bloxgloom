//! Opt-in CPU measurement, separate from world-only and character-only GPU
//! benchmarks. Includes resident/revision lookups, geometry, and output allocation.
use super::*;
use crate::world::{CHUNK_SIZE, CHUNK_VOLUME, Chunk, ChunkKey, STONE, world_to_chunk};
use std::{collections::HashMap, hint::black_box, time::Instant};

#[test]
#[ignore = "run with cargo test --release contact_shadow_cpu_128 -- --ignored --nocapture"]
fn contact_shadow_cpu_128() {
    let mut chunks = HashMap::new();
    for z in -1..=1 {
        for x in -1..=1 {
            if x == 1 && z == 1 {
                continue; // Unknown streamed chunk must fail closed.
            }
            let key = ChunkKey { x, y: 0, z };
            let mut blocks = vec![AIR; CHUNK_VOLUME];
            for local_z in 0..CHUNK_SIZE {
                for local_x in 0..CHUNK_SIZE {
                    let wx = x * CHUNK_SIZE as i32 + local_x as i32;
                    let wz = z * CHUNK_SIZE as i32 + local_z as i32;
                    // Mix flat surfaces with ledges, holes, and one-block walls.
                    if (wx + wz).rem_euclid(7) != 0 {
                        blocks[Chunk::index([local_x, 0, local_z]).unwrap()] = STONE;
                    }
                    if wx.rem_euclid(11) == 0 {
                        blocks[Chunk::index([local_x, 1, local_z]).unwrap()] = STONE;
                    }
                }
            }
            chunks.insert(key, Chunk::from_blocks(key, 0, blocks));
        }
    }
    // Mirror the client's two revision lookups for occupied receivers, including
    // one mesh that has not caught up with its newest authoritative snapshot.
    let revisions: HashMap<_, _> = chunks.keys().copied().map(|key| (key, 3u64)).collect();
    let mut displayed = revisions.clone();
    displayed.insert(ChunkKey { x: 1, y: 0, z: 0 }, 2);
    let avatars: Vec<_> = (0..MAX_CHARACTERS)
        .map(|i| {
            let mut avatar = super::tests::avatar(Vec3::new(
                (i % 16) as f32 * 1.5 - 4.05,
                1.0 + (i % 3) as f32 * 0.2,
                (i / 16) as f32 * 2.5 + 0.95,
            ));
            avatar.airborne = i % 3 != 0;
            avatar
        })
        .collect();
    let eye = Vec3::new(8.0, 5.0, 8.0);
    let mut samples = Vec::with_capacity(2000);
    let mut patch_count = 0;
    for frame in 0..2100 {
        let start = Instant::now();
        let result = patches(
            black_box(&avatars),
            eye,
            crate::content::catalog(),
            |x, y, z| {
                let (key, local) = world_to_chunk(x, y, z);
                let id = chunks.get(&key)?.block(local)?;
                if id != AIR && displayed.get(&key)? != revisions.get(&key)? {
                    return None;
                }
                Some(id)
            },
        );
        patch_count = result.len();
        black_box(result);
        if frame >= 100 {
            samples.push(start.elapsed().as_secs_f64() * 1_000_000.0);
        }
    }
    samples.sort_by(f64::total_cmp);
    assert!(patch_count > 0 && patch_count <= MAX_PATCHES);
    println!(
        "contact shadow CPU: 128 players, {patch_count} floor patches ({} bytes), median {:.2} us, p95 {:.2} us; mixed resident ledges/holes/walls/missing chunk + stale-mesh revision gate; excludes GPU/upload",
        patch_count * std::mem::size_of::<Patch>(),
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100]
    );
}
