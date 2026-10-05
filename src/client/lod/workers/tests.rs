use super::*;
use crate::lod::{Column, Interval, LodTile, Span, TILE_COLUMNS};

#[test]
fn pooled_meshes_preserve_identity_trace_and_release_catalog_after_completion() {
    let catalog = Arc::new(Catalog::builtins());
    let mut pool = Worker::with_workers(catalog.clone(), 4);
    let requested = Instant::now();
    for x in 0..4 {
        let key = TileKey { level: 1, x, z: -1 };
        let tile = Arc::new(LodTile {
            key,
            revision: 9,
            geometric_error: 0,
            columns: (0..TILE_COLUMNS)
                .map(|index| Column {
                    coverage: vec![Interval { bottom: 0, top: 32 }],
                    spans: vec![Span {
                        bottom: 0,
                        top: 10 + index as i32 % 3,
                        state: crate::world::STONE,
                        sky: 15,
                        glow: 0,
                    }],
                })
                .collect(),
        });
        assert!(pool.submit(Job {
            tile,
            neighbors: vec![],
            generation: x as u64 + 10,
            submitted: Instant::now(),
            trace: Some(crate::lod::loading::ClientTrace {
                request: x as u64 + 100,
                requested,
                received: requested,
                queued: requested
            }),
        }));
    }
    let mut keys = std::collections::HashSet::new();
    for _ in 0..4 {
        let result = pool.results.recv_timeout(Duration::from_secs(20)).unwrap();
        assert_eq!(result.generation, result.key.x as u64 + 10);
        assert!(keys.insert(result.key));
        let mesh = result.mesh.unwrap();
        assert_eq!(mesh.key, result.key);
        assert_eq!(mesh.revision, 9);
        assert_eq!(mesh.loading.unwrap().request, result.key.x as u64 + 100);
        assert!(result.queue_time + result.material_time + result.mesh_time <= requested.elapsed());
    }
    pool.stop();
    assert_eq!(
        Arc::strong_count(&catalog),
        1,
        "idle lanes do not own the catalog"
    );
}
