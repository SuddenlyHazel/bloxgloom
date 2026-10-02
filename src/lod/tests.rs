use super::*;
use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, STONE, WOOD};

fn chunk(y: i32, fill: crate::world::BlockId) -> Chunk {
    Chunk::from_blocks(ChunkKey { x: -1, y, z: -1 }, 1, vec![fill; CHUNK_VOLUME])
}
#[test]
fn euclidean_keys_and_checked_edges() {
    let key = TileKey::containing(0, -1, -33).unwrap();
    assert_eq!((key.x, key.z), (-1, -2));
    assert_eq!(key.bounds(), Some([-32, -64, 0, -32]));
    let parent = key.parent().unwrap();
    assert!(parent.children().unwrap().contains(&key));
    assert!(
        TileKey {
            level: 21,
            x: 0,
            z: 0
        }
        .bounds()
        .is_none()
    );
    assert!(
        TileKey {
            level: 0,
            x: i32::MAX,
            z: 0
        }
        .bounds()
        .is_none()
    );
}
#[test]
fn empty_is_known_missing_is_not_and_seams_join() {
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 0,
        x: -1,
        z: -1,
    };
    let tile = extract(key, 1, &[chunk(0, AIR)], &catalog).unwrap();
    assert!(tile.columns[16 + 16 * 32].known(0, 16));
    assert!(tile.columns[16 + 16 * 32].spans.is_empty());
    assert!(!tile.columns[0].known(0, 16));
    let tile = extract(key, 2, &[chunk(0, STONE), chunk(1, STONE)], &catalog).unwrap();
    let c = &tile.columns[16 + 16 * 32];
    assert_eq!(c.coverage, vec![Interval { bottom: 0, top: 32 }]);
    assert_eq!(c.spans.len(), 1);
    assert_eq!((c.spans[0].bottom, c.spans[0].top), (0, 32));
    assert_eq!(c.spans[0].sky, 0); // Unobserved air above is not full skylight.
}
#[test]
fn bridges_and_sealed_cavities_survive_deterministic_reduction() {
    let ground = Span {
        bottom: 0,
        top: 4,
        state: STONE,
        sky: 0,
        glow: 0,
    };
    let bridge = Span {
        bottom: 12,
        top: 13,
        state: WOOD,
        sky: 15,
        glow: 0,
    };
    let column = Column {
        coverage: vec![Interval { bottom: 0, top: 16 }],
        spans: vec![ground, bridge],
    };
    let air = Column {
        coverage: column.coverage.clone(),
        spans: vec![],
    };
    let first = reduce::merge(&[&column, &air, &air, &air]);
    assert_eq!(first.spans, vec![ground, bridge]);
    assert_eq!(first, reduce::merge(&[&air, &air, &column, &air]));
    assert_eq!(
        reduce::merge(&[&column, &Column::default()]),
        Column::default()
    );
    let ceiling = Span {
        bottom: 8,
        top: 16,
        state: STONE,
        sky: 0,
        glow: 0,
    };
    let sealed = Column {
        coverage: column.coverage,
        spans: vec![ground, ceiling],
    };
    let reduced = reduce::merge(&[&sealed, &sealed, &sealed, &sealed]);
    assert_eq!(reduced.spans, vec![ground, ceiling]);
    assert!(reduced.spans.iter().all(|s| s.sky == 0));
}
#[test]
fn parent_maps_child_quadrants_and_rejects_wrong_children() {
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 1,
        x: -1,
        z: 0,
    };
    let keys = key.children().unwrap();
    let mut children: Vec<_> = keys
        .into_iter()
        .map(|key| LodTile {
            key,
            revision: 1,
            columns: vec![Column::default(); TILE_COLUMNS],
            geometric_error: 0,
        })
        .collect();
    for z in 0..2 {
        for x in 0..2 {
            children[3].columns[x + z * 32] = Column {
                coverage: vec![Interval {
                    bottom: -16,
                    top: 16,
                }],
                spans: vec![Span {
                    bottom: 1,
                    top: 3,
                    state: WOOD,
                    sky: 0,
                    glow: 0,
                }],
            };
        }
    }
    let refs = [&children[0], &children[1], &children[2], &children[3]];
    let tile = reduce_parent(key, 2, refs, &catalog).unwrap();
    assert_eq!(tile.columns[16 + 16 * 32].spans[0].state, WOOD);
    assert!(tile.columns[0].coverage.is_empty());
    assert!(reduce_parent(key, 2, [refs[1], refs[0], refs[2], refs[3]], &catalog).is_err());
}
#[test]
fn excessive_geometry_and_illegal_states_are_rejected() {
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 0,
        x: 0,
        z: 0,
    };
    let mut tile = LodTile {
        key,
        revision: 1,
        columns: vec![Column::default(); TILE_COLUMNS],
        geometric_error: 0,
    };
    tile.columns[0] = Column {
        coverage: vec![Interval {
            bottom: 0,
            top: 100,
        }],
        spans: (0..33)
            .map(|i| Span {
                bottom: i * 2,
                top: i * 2 + 1,
                state: STONE,
                sky: 0,
                glow: 0,
            })
            .collect(),
    };
    assert!(tile.validate(&catalog).is_err());
    tile.columns[0].spans.truncate(1);
    tile.columns[0].spans[0].state = crate::content::BlockStateId(u32::MAX);
    assert!(tile.validate(&catalog).is_err());
}
#[test]
fn coarse_extraction_requires_every_horizontal_sample() {
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 1,
        x: -1,
        z: -1,
    };
    let tile = extract(key, 1, &[chunk(0, STONE)], &catalog).unwrap();
    assert!(tile.columns[24 + 24 * 32].known(0, 16));
    assert!(!tile.columns[23 + 24 * 32].known(0, 16));
}

#[test]
fn snapshot_order_preserves_trees_bridges_and_unknown_vertical_gaps() {
    use crate::world::{LEAVES, RED_FLOWER};
    let catalog = Catalog::builtins();
    let key = TileKey {
        level: 0,
        x: -1,
        z: -1,
    };
    let mut blocks = vec![AIR; CHUNK_VOLUME];
    for z in 0..16 {
        for x in 0..16 {
            blocks[Chunk::index([x, 0, z]).unwrap()] = STONE;
            blocks[Chunk::index([x, 12, z]).unwrap()] = WOOD;
        }
    }
    blocks[Chunk::index([0, 15, 0]).unwrap()] = LEAVES;
    blocks[Chunk::index([1, 15, 0]).unwrap()] = RED_FLOWER;
    let bridge = Chunk::from_blocks(ChunkKey { x: -1, y: 0, z: -1 }, 1, blocks);
    let high = chunk(3, STONE);
    let first = extract(key, 1, &[bridge.clone(), high.clone()], &catalog).unwrap();
    let second = extract(key, 1, &[high, bridge], &catalog).unwrap();
    assert_eq!(first, second);
    let tree = &first.columns[16 + 16 * 32];
    assert_eq!(tree.spans.len(), 4);
    assert!(tree.spans.iter().any(|s| s.state == LEAVES));
    assert!(!tree.known(16, 48));
    assert!(
        first.columns[17 + 16 * 32]
            .spans
            .iter()
            .all(|s| s.state != RED_FLOWER)
    );
    assert!(extract(key, 1, &[chunk(0, AIR), chunk(0, STONE)], &catalog).is_err());
}

#[test]
fn hostile_vertical_extent_cannot_overflow_mesh_dimensions() {
    let catalog = Catalog::builtins();
    let mut tile = LodTile {
        key: TileKey {
            level: 0,
            x: 0,
            z: 0,
        },
        revision: 1,
        columns: vec![Column::default(); TILE_COLUMNS],
        geometric_error: 0,
    };
    tile.columns[0] = Column {
        coverage: vec![Interval {
            bottom: i32::MIN,
            top: i32::MAX,
        }],
        spans: vec![Span {
            bottom: i32::MIN,
            top: i32::MAX,
            state: STONE,
            sky: 0,
            glow: 0,
        }],
    };
    assert!(tile.validate(&catalog).is_err());
}
