use super::*;
use crate::world::{AIR, MAX_GENERATED_HEIGHT, STONE, World};
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
fn spatial_queries_match_brute_force_across_negative_bucket_edges() {
    let mut drops = Drops::new();
    let edges = [
        -65.0, -32.01, -32.0, -16.01, -16.0, -0.01, 0.0, 15.99, 16.0, 32.0, 64.0,
    ];
    for (index, &x) in edges.iter().enumerate() {
        for (depth, &z) in edges.iter().enumerate() {
            let id = (index * edges.len() + depth + 1) as u64;
            insert_entry(
                &mut drops,
                id,
                [x, (index as f32 - 4.0) * 15.99, z],
                2,
                1,
                Duration::ZERO,
                Duration::ZERO,
            );
        }
    }
    assert_spatial_members_match_entries(&drops);

    let centers = [
        [-16.0, -16.0, -16.0],
        [-0.1, -0.1, -0.1],
        [0.0, 0.0, 0.0],
        [31.9, 16.0, -32.0],
    ];
    for center in centers {
        let expected_nearby: Vec<_> = {
            let mut items: Vec<_> = drops
                .entries
                .values()
                .filter(|entry| distance_sq(entry.item.position, center) <= VIEW_RANGE_SQ)
                .map(Entry::snapshot)
                .collect();
            items.sort_by(|a, b| {
                distance_sq(a.position, center)
                    .total_cmp(&distance_sq(b.position, center))
                    .then(a.id.cmp(&b.id))
            });
            items.truncate(256);
            items
        };
        let expected_pickups: Vec<_> = {
            let mut items: Vec<_> = drops
                .entries
                .values()
                .filter(|entry| {
                    let age = entry.age();
                    age >= entry.pickup_delay
                        && age < LIFETIME
                        && distance_sq(entry.item.position, center) <= PICKUP_RANGE_SQ
                })
                .map(Entry::snapshot)
                .collect();
            items.sort_by_key(|item| item.id);
            items.truncate(256);
            items
        };
        assert_eq!(
            stable_items(&drops.nearby(center)),
            stable_items(&expected_nearby)
        );
        assert_eq!(
            stable_items(&drops.pickup_candidates(center)),
            stable_items(&expected_pickups)
        );
    }
}

#[test]
fn wake_near_uses_negative_coordinate_buckets_and_exact_overlap() {
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [-0.1, 11.18, -0.1],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        2,
        [0.19, 11.18, -0.1],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    drops.active.clear();
    drops.wake_near([-1, 10, -1]);
    assert!(drops.active.contains(&1));
    assert!(!drops.active.contains(&2));
}

#[test]
fn falling_drop_moves_between_vertical_spatial_buckets() {
    let root = temp_root("bloxgloom-drop-index-motion");
    fs::create_dir_all(&root).unwrap();
    let mut world = World::new(19, root.clone()).unwrap();
    world.get_block(1, 511, 1).unwrap();
    let mut drops = Drops::new();
    drops.spawn([1.0, 512.05, 1.0], item(2), 1, Duration::ZERO);
    assert_eq!(
        drops
            .spatial
            .query_aabb([1.0, 512.05, 1.0], [1.0, 512.05, 1.0]),
        [1]
    );
    let result = drops.step(&world, Duration::from_millis(100));
    assert!(result.missing_chunks.is_empty());
    let new_position = drops.entries[&1].item.position;
    assert!(new_position[1] < 512.0);
    assert!(
        drops
            .spatial
            .query_aabb([1.0, 512.05, 1.0], [1.0, 512.05, 1.0])
            .is_empty()
    );
    assert_eq!(drops.spatial.query_aabb(new_position, new_position), [1]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn physics_defers_on_missing_chunks_and_caps_requests_at_64() {
    let root = temp_root("bloxgloom-drop-index-deferred");
    fs::create_dir_all(&root).unwrap();
    let world = World::new(19, root.clone()).unwrap();
    let mut drops = Drops::new();
    for id in 1..=100 {
        insert_entry(
            &mut drops,
            id,
            [id as f32 * 32.0, 512.05, 1.0],
            2,
            1,
            Duration::ZERO,
            Duration::ZERO,
        );
    }
    let before: Vec<_> = drops
        .entries
        .iter()
        .map(|(&id, entry)| (id, entry.item.position))
        .collect();
    let result = drops.step(&world, Duration::from_millis(100));
    assert!(!result.moved);
    assert!(!result.landed);
    assert_eq!(result.missing_chunks.len(), 64);
    assert!(
        result
            .missing_chunks
            .windows(2)
            .all(|pair| (pair[0].x, pair[0].y, pair[0].z) < (pair[1].x, pair[1].y, pair[1].z))
    );
    for (id, position) in before {
        assert_eq!(drops.entries[&id].item.position, position);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_terrain_defers_only_the_affected_drop() {
    let root = temp_root("bloxgloom-drop-index-fairness");
    fs::create_dir_all(&root).unwrap();
    let mut world = World::new(19, root.clone()).unwrap();
    world.get_block(1, 511, 1).unwrap();
    let mut drops = Drops::new();
    insert_entry(
        &mut drops,
        1,
        [1.0, 512.05, 1.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );
    insert_entry(
        &mut drops,
        2,
        [1_000.0, 512.05, 1.0],
        2,
        1,
        Duration::ZERO,
        Duration::ZERO,
    );

    let loaded_before = drops.entries[&1].item.position[1];
    let missing_before = drops.entries[&2].item.position[1];
    let result = drops.step(&world, Duration::from_millis(100));

    assert!(result.moved);
    assert!(!result.missing_chunks.is_empty());
    assert!(drops.entries[&1].item.position[1] < loaded_before);
    assert_eq!(drops.entries[&2].item.position[1], missing_before);
    assert!(
        drops.active.contains(&2),
        "deferred drop must remain active"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn airborne_drop_lands_and_falls_again_when_support_is_removed() {
    let root = temp_root("bloxgloom-drop-gravity");
    fs::create_dir_all(&root).unwrap();
    let mut world = World::new(19, root.clone()).unwrap();
    let ground = MAX_GENERATED_HEIGHT + 12;
    world.edit(0, ground, 0, STONE).unwrap();
    let mut drops = Drops::open(&root).unwrap();
    drops.spawn([0.5, ground as f32 + 5.0, 0.5], item(2), 1, Duration::ZERO);
    assert_eq!(drops.active_len(), 1);
    for _ in 0..100 {
        let result = drops.step(&world, Duration::from_millis(20));
        assert!(result.missing_chunks.is_empty());
    }
    let landed_y = ground as f32 + 1.0 + DROP_RADIUS;
    assert!((drops.entries[&1].item.position[1] - landed_y).abs() < 0.001);
    assert!(drops.active.is_empty());
    assert_eq!(drops.active_len(), 0);
    assert_eq!(drops.entries.len(), 1, "settled drops remain live entries");
    drops.save().unwrap();
    let loaded = Drops::open(&root).unwrap();
    assert!((loaded.entries[&1].item.position[1] - landed_y).abs() < 0.001);

    world.edit(0, ground, 0, AIR).unwrap();
    drops.wake_near([0, ground, 0]);
    assert!(drops.active.contains(&1));
    let result = drops.step(&world, Duration::from_millis(20));
    assert!(result.missing_chunks.is_empty());
    assert!(drops.entries[&1].item.position[1] < landed_y);
    assert_spatial_members_match_entries(&drops);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "manual bucket-size comparison; run with --ignored --nocapture"]
fn benchmark_spatial_bucket_sizes_for_pickup_and_nearby_workloads() {
    let mut clustered = Vec::with_capacity(50_000);
    for id in 0..50_000u32 {
        let x = ((id.wrapping_mul(17)) % 121) as f32 - 60.0;
        let y = 96.0 + ((id.wrapping_mul(13)) % 49) as f32;
        let z = ((id.wrapping_mul(29)) % 121) as f32 - 60.0;
        clustered.push([x, y, z]);
    }
    let pickup_queries: Vec<_> = (0..256u32)
        .map(|id| {
            [
                (id % 17) as f32 * 3.5 - 28.0,
                96.0 + (id % 11) as f32 * 4.0,
                (id % 19) as f32 * 3.0 - 27.0,
            ]
        })
        .collect();
    benchmark_sizes("clustered pickup", &clustered, &pickup_queries, 2.25);

    let mut spread = Vec::with_capacity(50_000);
    for id in 0..50_000u32 {
        let x = ((id.wrapping_mul(2_654_435_761) % 65_536) as f32 - 32_768.0) * 0.25;
        let y = ((id.wrapping_mul(2_246_822_519) % 4_096) as f32 - 2_048.0) * 0.25;
        let z = ((id.wrapping_mul(3_266_489_917) % 65_536) as f32 - 32_768.0) * 0.25;
        spread.push([x, y, z]);
    }
    let nearby_queries: Vec<_> = (0..256i32)
        .map(|id| {
            [
                (id % 16) as f32 * 800.0 - 6_000.0,
                (id % 9) as f32 * 80.0 - 320.0,
                (id % 13) as f32 * 900.0 - 5_400.0,
            ]
        })
        .collect();
    benchmark_sizes("spread nearby", &spread, &nearby_queries, 64.0);
}

fn benchmark_sizes(label: &str, positions: &[[f32; 3]], queries: &[[f32; 3]], radius: f32) {
    let mut result_ids = None;
    for bucket_size in [16, 32] {
        let mut spatial_index = spatial::DropSpatialIndex::with_bucket_size(bucket_size);
        for (ordinal, &position) in positions.iter().enumerate() {
            spatial_index.insert(ordinal as u64 + 1, position);
        }
        let mut candidate_total = 0usize;
        let mut match_total = 0usize;
        let mut matched_ids = Vec::new();
        let start = Instant::now();
        for _ in 0..3 {
            for &query in queries {
                let min = query.map(|coordinate| coordinate - radius);
                let max = query.map(|coordinate| coordinate + radius);
                let candidates = spatial_index.query_aabb(min, max);
                candidate_total += candidates.len();
                let matches: Vec<_> = candidates
                    .iter()
                    .filter(|&&id| {
                        distance_sq(positions[(id - 1) as usize], query) <= radius * radius
                    })
                    .copied()
                    .collect();
                match_total += matches.len();
                matched_ids.extend(matches);
                std::hint::black_box(candidates);
            }
        }
        let elapsed = start.elapsed();
        if let Some(previous) = &result_ids {
            assert_eq!(
                *previous, matched_ids,
                "bucket size changed exact result IDs"
            );
        } else {
            result_ids = Some(matched_ids);
        }
        eprintln!(
            "{label}: bucket={bucket_size} elapsed_ms={:.2} broad_candidates={} exact_matches={}",
            elapsed.as_secs_f64() * 1_000.0,
            candidate_total,
            match_total
        );
    }
}
