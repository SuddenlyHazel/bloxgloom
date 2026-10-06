use super::*;
use crate::world::{AIR, CHUNK_VOLUME, Chunk, ChunkKey, STONE, WATER};

fn chunk(id: world::BlockId) -> Chunk {
    Chunk::from_blocks(ChunkKey { x: -1, y: 0, z: -2 }, 7, vec![id; CHUNK_VOLUME])
}

#[test]
fn authoritative_occupancy_preserves_voxel_order_and_compacts_full_water() {
    let catalog = content::catalog();
    assert_eq!(Occupancy::from_chunk(&chunk(AIR), catalog), None);
    let full = Occupancy::from_chunk(&chunk(WATER), catalog).unwrap();
    assert_eq!(full.class, 1);
    assert!(full.mask.is_empty());
    assert_eq!(full.byte_len(), 4);
    let mut mixed = chunk(AIR);
    let positions = [[0, 0, 0], [15, 0, 1], [1, 4, 3], [15, 15, 15]];
    for position in positions {
        mixed.blocks.set(Chunk::index(position).unwrap(), WATER);
    }
    // Opaque bottoms do not turn adjacent dry or enclosed voxels into water.
    mixed.blocks.set(Chunk::index([1, 3, 3]).unwrap(), STONE);
    let exact = Occupancy::from_chunk(&mixed, catalog).unwrap();
    assert_eq!(exact.class, 2);
    assert_eq!(exact.mask.len(), 128);
    assert_eq!(exact.byte_len(), 516);
    assert_eq!(
        exact.mask.iter().map(|word| word.count_ones()).sum::<u32>(),
        4
    );
    for position in positions {
        let index = Chunk::index(position).unwrap();
        assert_ne!(exact.mask[index / 32] & (1 << (index % 32)), 0);
    }
    mixed.blocks.set(Chunk::index([1, 4, 3]).unwrap(), AIR);
    let edited = Occupancy::from_chunk(&mixed, catalog).unwrap();
    assert_eq!(
        edited
            .mask
            .iter()
            .map(|word| word.count_ones())
            .sum::<u32>(),
        3
    );
}

#[test]
fn coarse_water_keeps_server_wet_intervals_and_unknown_cave_gaps() {
    use crate::lod::{Column, Interval, LodTile, Span, TILE_COLUMNS, TileKey};
    let catalog = content::catalog();
    let mut columns = vec![Column::default(); TILE_COLUMNS];
    columns[0] = Column {
        coverage: vec![
            Interval { bottom: -8, top: 3 },
            Interval { bottom: 8, top: 20 },
        ],
        spans: vec![
            Span {
                bottom: -8,
                top: -4,
                state: STONE,
                sky: 0,
                glow: 0,
            },
            Span {
                bottom: -4,
                top: 0,
                state: WATER,
                sky: 15,
                glow: 0,
            },
            Span {
                bottom: 0,
                top: 3,
                state: WATER,
                sky: 15,
                glow: 0,
            },
            Span {
                bottom: 8,
                top: 11,
                state: WATER,
                sky: 15,
                glow: 0,
            },
        ],
    };
    columns[1] = Column {
        coverage: vec![Interval { bottom: 0, top: 20 }],
        spans: vec![],
    };
    let tile = LodTile {
        key: TileKey {
            level: 2,
            x: -2,
            z: -1,
        },
        revision: 9,
        columns,
        trees: vec![],
        geometric_error: 4,
    };
    let coarse = CoarseTile::from_lod(&tile, catalog).unwrap();
    assert_eq!(coarse.key, tile.key);
    assert_eq!(coarse.columns.len(), TILE_COLUMNS);
    assert_eq!(
        coarse.columns[0].water,
        [
            Interval { bottom: -4, top: 3 },
            Interval { bottom: 8, top: 11 }
        ]
    );
    assert_eq!(coarse.columns[0].coverage, tile.columns[0].coverage);
    assert!(coarse.columns[1].water.is_empty() && !coarse.columns[1].coverage.is_empty());
    assert!(
        coarse.columns[2].coverage.is_empty(),
        "unknown columns must not become dry"
    );
    assert!(coarse.byte_len() < 17 * 1024);
    let mut malformed = tile;
    malformed.columns[0].spans[2].top = 5;
    assert!(
        CoarseTile::from_lod(&malformed, catalog).is_none(),
        "wet ranges cannot bridge unknown gaps"
    );
}

#[test]
fn actual_fluid_mesh_preserves_outward_faces_and_known_neighbor_retirement() {
    let catalog = content::catalog();
    let water = chunk(WATER);
    let mesh = crate::render::mesh::mesh_chunk(&water);
    let mut triangles = Vec::new();
    append(&mesh, catalog, &mut triangles);
    assert_eq!(
        triangles.len(),
        4,
        "unknown lateral neighbors do not prove walls"
    );
    for triangle in &triangles {
        assert_eq!(
            triangle.surface_flags,
            surface::WATER | surface::COARSE_COLOR | surface::NO_WIND
        );
        assert_eq!(triangle.a[3], 0.0, "water never borrows an opaque material");
        assert_eq!(triangle.c[3], 0.0, "water is not an alpha-cutout card");
        let a = glam::Vec3::from_slice(&triangle.a);
        let b = glam::Vec3::from_slice(&triangle.b);
        let c = glam::Vec3::from_slice(&triangle.c);
        assert!(
            (b - a)
                .cross(c - a)
                .normalize()
                .dot(glam::Vec3::from_slice(&triangle.normal))
                > 0.999,
            "interface winding must preserve entry/exit orientation"
        );
    }
    // A real loaded air peer certifies the negative-X shoreline. Replacing
    // that peer with water must retire just this interface at the same seam.
    let peer_key = ChunkKey {
        x: water.key.x - 1,
        ..water.key
    };
    let mut known = std::collections::HashMap::from([
        (water.key, std::sync::Arc::new(water.clone())),
        (
            peer_key,
            std::sync::Arc::new(Chunk::from_blocks(peer_key, 1, vec![AIR; CHUNK_VOLUME])),
        ),
    ]);
    let with_peer = |known: &std::collections::HashMap<_, _>| {
        let light = crate::lighting::LightField::build_with_catalog(water.key, known, 1, catalog);
        let mesh =
            crate::render::mesh::mesh_chunk_lit_with_neighbors(&water, &light, 1, catalog, known);
        let mut result = Vec::new();
        append(&mesh, catalog, &mut result);
        result
    };
    let shoreline = with_peer(&known);
    let certified_area: f32 = shoreline
        .iter()
        .filter(|t| t.normal[0] < -0.5)
        .map(|t| {
            let a = glam::Vec3::from_slice(&t.a);
            let b = glam::Vec3::from_slice(&t.b);
            let c = glam::Vec3::from_slice(&t.c);
            assert_eq!(a.x, -16.0);
            assert_eq!(b.x, -16.0);
            assert_eq!(c.x, -16.0);
            (b - a).cross(c - a).length() * 0.5
        })
        .sum();
    assert_eq!(
        certified_area, 256.0,
        "lighting may split greedy quads, never shoreline coverage"
    );
    assert!(
        shoreline
            .iter()
            .all(|t| t.normal[0] <= 0.0 && t.normal[2] == 0.0)
    );
    known.insert(
        peer_key,
        std::sync::Arc::new(Chunk::from_blocks(peer_key, 2, vec![WATER; CHUNK_VOLUME])),
    );
    let joined = with_peer(&known);
    assert!(joined.iter().all(|t| t.normal[0].abs() < 0.5));
    let air = chunk(AIR);
    let empty = crate::render::mesh::mesh_chunk(&air);
    let mut removed = Vec::new();
    append(&empty, catalog, &mut removed);
    assert!(removed.is_empty());
}
