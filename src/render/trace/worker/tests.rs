use super::*;
use crate::render::trace::scene::Triangle;
use std::time::{Duration, Instant};

mod gpu;

fn geometry(x: f32) -> Arc<Chunk> {
    Arc::new(Chunk {
        water: None,
        coarse_water: None,

        key: None,
        triangles: vec![Triangle {
            a: [x, 0.0, 0.0, 0.0],
            b: [x + 1.0, 0.0, 0.0, 0.0],
            c: [x, 1.0, 0.0, 0.0],
            uv_ab: [0.0; 4],
            uv_c: [0.0; 2],
            surface_color: 0,
            surface_flags: 0,
            normal: [0.0, 0.0, 1.0, 0.0],
        }],
    })
}

fn latest(worker: &Worker) -> Scene {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(scene) = worker.poll() {
            return scene;
        }
        assert!(
            Instant::now() < deadline,
            "ray-scene worker did not publish revision {}",
            worker.revision
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn queued_replacements_and_removals_never_resurrect_old_geometry() {
    let mut worker = Worker::new(u64::MAX);
    let a = ChunkKey { x: -1, y: 2, z: 0 };
    let b = ChunkKey { x: 0, y: 2, z: 0 };
    worker.set(a, Some(geometry(-16.0)));
    worker.set(b, Some(geometry(0.0)));
    worker.set(a, Some(geometry(-12.0)));
    worker.set(b, None);
    assert_eq!(worker.revision, 4);
    // This must be true whether the thread batches all updates, or completes
    // old builds between them: only the current world revision can be accepted.
    let scene = latest(&worker);
    assert_eq!(scene.triangles.len(), 1);
    assert_eq!(scene.triangles[0].a[0], -12.0);
    assert!(!scene.nodes.is_empty());
    worker.set(a, None);
    let empty = latest(&worker);
    assert!(empty.triangles.is_empty() && empty.nodes.is_empty());
    assert!(worker.poll().is_none());
}

#[test]
fn poll_rejects_stale_results_on_both_sides_of_current_result() {
    let (updates, _receive_updates) = mpsc::channel();
    let ready = Arc::new(Mutex::new(None));
    let worker = Worker {
        updates,
        ready: ready.clone(),
        revision: 3,
        gpu_configured: false,
        latest_revision: Arc::new(AtomicU64::new(3)),
    };
    for (revision, x) in [(2, -16.0), (3, 8.0), (1, -32.0)] {
        publish(
            &ready,
            Ready {
                revision,
                scene: Scene::build([geometry(x)]),
                gpu: None,
                lod_pages: Vec::new(),
            },
        );
    }
    let current = worker
        .poll()
        .expect("current result remains available despite later stale delivery");
    assert_eq!(current.triangles.len(), 1);
    assert_eq!(current.triangles[0].a[0], 8.0);
    assert!(worker.poll().is_none());
    publish(
        &ready,
        Ready {
            revision: 2,
            scene: Scene::build([geometry(-16.0)]),
            gpu: None,
            lod_pages: Vec::new(),
        },
    );
    assert!(
        worker.poll().is_none(),
        "old removed geometry must not become active again"
    );
}

#[test]
fn oversized_triangle_total_skips_build_and_recovers_after_removal() {
    // One triangle is exactly at the per-binding quota; a second must be
    // rejected before the full scene is copied, regardless of update batching.
    let mut worker = Worker::new(std::mem::size_of::<Triangle>() as u64);
    let a = ChunkKey { x: 0, y: 0, z: 0 };
    let b = ChunkKey { x: 1, y: 0, z: 0 };
    worker.set(a, Some(geometry(0.0)));
    let exact = latest(&worker);
    assert_eq!(exact.triangles.len(), 1);
    assert!(exact.fits(std::mem::size_of::<Triangle>() as u64));
    worker.set(b, Some(geometry(16.0)));
    let rejected = latest(&worker);
    assert!(rejected.triangles.is_empty() && rejected.nodes.is_empty());
    worker.set(b, None);
    assert_eq!(latest(&worker).triangles.len(), 1);
}

#[test]
fn latest_mailbox_preserves_revision_wrap_and_replaces_full_results() {
    let (updates, _receive_updates) = mpsc::channel();
    let ready = Arc::new(Mutex::new(None));
    let worker = Worker {
        updates,
        ready: ready.clone(),
        revision: 0,
        gpu_configured: false,
        latest_revision: Arc::new(AtomicU64::new(0)),
    };
    for (revision, x) in [
        (u64::MAX - 1, 1.0),
        (u64::MAX, 2.0),
        (0, 3.0),
        (u64::MAX - 1, 4.0),
    ] {
        publish(
            &ready,
            Ready {
                revision,
                scene: Scene::build([geometry(x)]),
                gpu: None,
                lod_pages: Vec::new(),
            },
        );
    }
    assert_eq!(ready.lock().unwrap().as_ref().unwrap().revision, 0);
    let scene = worker.poll().unwrap();
    assert_eq!(scene.triangles[0].a[0], 3.0);
    assert!(
        ready.lock().unwrap().is_none(),
        "only one completed scene is retained"
    );
}

#[test]
#[ignore = "isolated CPU rebuild benchmark; run in release with --ignored --nocapture"]
fn real_mesher_worker_rebuild_benchmark() {
    use crate::world::{AIR, CHUNK_SIZE, CHUNK_VOLUME, DIRT, GRASS, STONE, TALL_GRASS};
    let mesh_started = Instant::now();
    let chunks: Vec<_> = (0..16)
        .map(|index| {
            let key = ChunkKey {
                x: index % 4,
                y: 0,
                z: index / 4,
            };
            let mut chunk = crate::world::Chunk {
                key,
                version: 1,
                blocks: vec![AIR; CHUNK_VOLUME].into(),
            };
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let height = 3 + (x * 3 + z * 5 + index as usize) % 10;
                    for y in 0..height {
                        let material = if y + 1 == height {
                            GRASS
                        } else if y + 3 >= height {
                            DIRT
                        } else {
                            STONE
                        };
                        chunk
                            .blocks
                            .set(crate::world::Chunk::index([x, y, z]).unwrap(), material);
                    }
                    if (x + z) % 7 == 0 {
                        chunk.blocks.set(
                            crate::world::Chunk::index([x, height, z]).unwrap(),
                            TALL_GRASS,
                        );
                    }
                }
            }
            (key, crate::render::mesh_chunk(&chunk).trace)
        })
        .collect();
    let mesh_time = mesh_started.elapsed();
    let expected_triangles: usize = chunks.iter().map(|(_, c)| c.triangles.len()).sum();
    assert!(
        expected_triangles > 10_000,
        "fixture exercises terrain and cutout meshing"
    );
    let mut timings = Vec::new();
    let mut scene_bytes = 0;
    for _ in 0..5 {
        let mut worker = Worker::new(u64::MAX);
        let started = Instant::now();
        for (key, chunk) in &chunks {
            worker.set(*key, Some(chunk.clone()));
        }
        let scene = latest(&worker);
        timings.push(started.elapsed());
        assert_eq!(scene.triangles.len(), expected_triangles);
        assert!(worker.poll().is_none());
        scene_bytes = scene.byte_len();
    }
    timings.sort();
    eprintln!(
        "real-mesher CPU benchmark: chunks={} triangles={expected_triangles} scene_bytes={scene_bytes} mesh_ms={:.2} worker_median_ms={:.2} worker_range_ms={:.2}..{:.2}",
        chunks.len(),
        mesh_time.as_secs_f64() * 1000.,
        timings[2].as_secs_f64() * 1000.,
        timings[0].as_secs_f64() * 1000.,
        timings[4].as_secs_f64() * 1000.
    );
}

fn latest_ready(worker: &Worker) -> Ready {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = worker.poll_ready() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "worker revision was not published"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn selected_lod_replacement_removal_preserves_near_and_revision() {
    let mut worker = Worker::new(4096);
    let near = ChunkKey { x: -1, y: 0, z: 0 };
    let tile = TileKey {
        level: 1,
        x: -1,
        z: 0,
    };
    worker.set(near, Some(geometry(-16.0)));
    worker.set_lod(tile, Some(geometry(-64.0)));
    worker.set_lod(tile, Some(geometry(-60.0)));
    let result = latest_ready(&worker);
    assert_eq!(result.revision, 3);
    assert_eq!(result.scene.triangles[0].a[0], -16.0);
    assert_eq!(result.lod_pages.len(), 1);
    assert_eq!(result.lod_pages[0].triangles[0].a[0], -60.0);
    assert!(result.lod_pages[0].coverage.is_empty());
    worker.set_lod(tile, None);
    let result = latest_ready(&worker);
    assert_eq!(result.revision, 4);
    assert_eq!(result.scene.triangles.len(), 1);
    assert!(result.lod_pages.is_empty());
}

#[test]
fn lod_page_overflow_rejects_the_whole_scene_and_recovers() {
    let mut worker = Worker::new(208);
    let tile = TileKey {
        level: 0,
        x: 0,
        z: 0,
    };
    worker.set(ChunkKey { x: 0, y: 0, z: 0 }, Some(geometry(0.0)));
    let too_large = Chunk {
        triangles: vec![geometry(64.0).triangles[0]; 5],
        ..Default::default()
    };
    worker.set_lod(tile, Some(Arc::new(too_large)));
    let result = latest_ready(&worker);
    assert!(result.scene.triangles.is_empty());
    assert!(result.lod_pages.is_empty());
    worker.set_lod(tile, Some(geometry(64.0)));
    let result = latest_ready(&worker);
    assert_eq!(result.scene.triangles.len(), 1);
    assert_eq!(result.lod_pages.len(), 1);
}

#[test]
fn medium_quota_is_atomic_and_loaded_empty_near_stays_eligible() {
    use crate::render::trace::scene::water::{CoarseColumn, CoarseTile, Occupancy};
    let key = ChunkKey { x: -1, y: 0, z: 0 };
    let tile = TileKey {
        level: 0,
        x: -1,
        z: 0,
    };
    let mut near = BTreeMap::new();
    near.insert(
        key,
        Arc::new(Chunk {
            key: Some(key),
            ..Default::default()
        }),
    );
    let (scene, pages) = assemble(&near, &BTreeMap::new(), 48).unwrap();
    assert!(scene.nodes.is_empty());
    assert!(eligible(&scene, &pages));
    assert!(!eligible(&Scene::default(), &[]));
    let mut lod = BTreeMap::new();
    lod.insert(
        tile,
        Arc::new(Chunk {
            coarse_water: Some(CoarseTile {
                key: tile,
                columns: vec![CoarseColumn {
                    coverage: vec![crate::lod::Interval { bottom: 0, top: 20 }],
                    water: vec![crate::lod::Interval {
                        bottom: 16,
                        top: 17,
                    }],
                }],
            }),
            ..Default::default()
        }),
    );
    // 48 loaded +16 medium header +32 directory +16 column +16 intervals.
    assert!(assemble(&near, &lod, 127).is_none());
    let (scene, _) = assemble(&near, &lod, 128).unwrap();
    assert_eq!(scene.coverage.len() * 4, 128);
    assert_eq!(scene.water_offset, 12);
    near.insert(
        key,
        Arc::new(Chunk {
            key: Some(key),
            water: Some(Occupancy {
                class: 1,
                mask: vec![],
            }),
            ..Default::default()
        }),
    );
    assert!(assemble(&near, &lod, 143).is_none());
    assert_eq!(
        assemble(&near, &lod, 144).unwrap().0.coverage.len() * 4,
        144
    );
}
