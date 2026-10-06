use super::*;
#[test]
fn paired_plants_share_the_same_rotation_and_width() {
    use crate::world::{AIR, CHUNK_SIZE, Chunk, ChunkKey};
    let catalog = crate::content::catalog();
    let lower = catalog
        .state_by_key("bloxgloom:large_fern[half=lower]")
        .unwrap();
    let upper = catalog
        .state_by_key("bloxgloom:large_fern[half=upper]")
        .unwrap();
    let mut chunk = Chunk {
        key: ChunkKey { x: -1, y: 0, z: 1 },
        version: 0,
        blocks: vec![AIR; CHUNK_SIZE.pow(3)].into(),
    };
    chunk.blocks.set(Chunk::index([3, 2, 4]).unwrap(), lower);
    chunk.blocks.set(Chunk::index([3, 3, 4]).unwrap(), upper);
    let mesh = crate::render::mesh::mesh_chunk(&chunk);
    let vertices: Vec<_> = mesh.cutout_vertices.chunks_exact(VERTEX_FLOATS).collect();
    assert_eq!(vertices.len(), 16);
    for (lower, upper) in vertices[..8].iter().zip(&vertices[8..]) {
        assert_eq!(lower[0], upper[0], "paired halves align in X");
        assert_eq!(lower[2], upper[2], "paired halves align in Z");
        assert!((upper[1] - lower[1] - 1.0).abs() < 0.00001);
    }
}
#[test]
fn botanical_variation_is_cell_stable_bounded_and_not_grid_aligned() {
    let cell = [-32.0, 41.0, 7.0];
    let a = plant_position(cell, 0.08, 0.08);
    assert_eq!(a, plant_position(cell, 0.08, 0.08));
    assert_ne!(a, plant_position([-31.0, 41.0, 7.0], 0.08, 0.08));
    let top: Vec<_> = leaf_cluster(cell, 1 << 3).collect();
    assert_eq!(top.len(), 3);
    assert!(
        top.iter().any(|c| c.normal.y > 0.80),
        "exposed canopy tops retain a slanted upward-facing sheet"
    );
    for x in -64..64 {
        let p = plant_position([x as f32, 33.0, -7.0], 0.08, 0.08);
        assert!(p.into_iter().all(|v| (-0.1..1.1).contains(&v)));
    }
    let cards: Vec<_> = leaf_cluster(cell, 63).collect();
    assert_eq!(cards, leaf_cluster(cell, 63).collect::<Vec<_>>());
    assert_ne!(
        cards,
        leaf_cluster([-31.0, 41.0, 7.0], 63).collect::<Vec<_>>()
    );
    for x in -64..64 {
        for card in leaf_cluster([x as f32, 33.0, -7.0], 63 | NEAR_TIMBER | ABOVE_CAP) {
            assert!(
                card.positions
                    .iter()
                    .all(|p| p.min_element() >= 0.0 && p.max_element() <= 1.0)
            );
            assert!(
                card.normal.y.abs() < 0.9999,
                "leaf cards must never use plant-root wind encoding"
            );
            let geometric = (card.positions[1] - card.positions[0])
                .cross(card.positions[2] - card.positions[0])
                .normalize();
            assert!(geometric.dot(card.normal) > 0.99999);
            assert!((card.normal.length() - 1.0).abs() < 0.00001);
            assert!(
                card.normal
                    .to_array()
                    .into_iter()
                    .filter(|v| v.abs() > 0.01)
                    .count()
                    >= 2
            );
        }
    }
}
fn empty_chunk(key: crate::world::ChunkKey) -> crate::world::Chunk {
    crate::world::Chunk {
        key,
        version: 0,
        blocks: vec![crate::world::AIR; crate::world::CHUNK_SIZE.pow(3)].into(),
    }
}

#[test]
fn dense_mixed_canopies_have_no_buried_geometry_and_keep_leaf_uvs() {
    use crate::world::{Chunk, ChunkKey, LEAVES};
    let catalog = crate::content::catalog();
    let cherry = catalog.state_by_key("bloxgloom:cherry_leaves").unwrap();
    let mut chunk = empty_chunk(ChunkKey { x: 0, y: 0, z: 0 });
    for y in 2..5 {
        for z in 2..5 {
            for x in 2..5 {
                chunk.blocks.set(
                    Chunk::index([x, y, z]).unwrap(),
                    if (x + y + z) % 2 == 0 { cherry } else { LEAVES },
                );
            }
        }
    }
    let mesh = crate::render::mesh::mesh_chunk(&chunk);
    // 6 face centers ×3 cards, 12 edges ×3, 8 corners ×4; no interior voxel.
    assert_eq!(mesh.cutout_indices.len() / 3, 172);
    for quad in mesh.cutout_vertices.chunks_exact(4 * VERTEX_FLOATS) {
        let vertices: Vec<_> = quad.chunks_exact(VERTEX_FLOATS).collect();
        let cell = vertices[0][..3]
            .iter()
            .map(|v| v.floor() as i32)
            .collect::<Vec<_>>();
        assert_ne!(cell, [3, 3, 3]);
        assert!(vertices.iter().all(|v| {
            v[..3]
                .iter()
                .zip(&cell)
                .all(|(p, c)| p.floor() as i32 == *c)
        }));
        assert_eq!(
            vertices.iter().map(|v| [v[6], v[7]]).collect::<Vec<_>>(),
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]
        );
        assert!(
            vertices
                .iter()
                .all(|v| v[8].floor() == vertices[0][8].floor())
        );
    }
    let scene = crate::render::trace::scene::Scene::build([mesh.trace.clone()]);
    assert_eq!(scene.triangles.len(), mesh.cutout_indices.len() / 3);
    let bounds = &scene.nodes[0];
    for vertex in mesh.cutout_vertices.chunks_exact(VERTEX_FLOATS) {
        for (axis, value) in vertex.iter().take(3).enumerate() {
            assert!(bounds.min[axis] <= value - 0.1199);
            assert!(bounds.max[axis] >= value + 0.1199);
        }
    }
    for triangle in &scene.triangles {
        let geometric = (Vec3::from_slice(&triangle.b[..3]) - Vec3::from_slice(&triangle.a[..3]))
            .cross(Vec3::from_slice(&triangle.c[..3]) - Vec3::from_slice(&triangle.a[..3]))
            .normalize();
        assert!(geometric.dot(Vec3::from_slice(&triangle.normal[..3])) > 0.99999);
    }
}

#[test]
fn loaded_neighbor_leaves_cull_buried_clusters_across_a_negative_chunk_seam() {
    use crate::world::{Chunk, ChunkKey, LEAVES};
    use std::{collections::HashMap, sync::Arc};
    let mut chunk = empty_chunk(ChunkKey { x: -1, y: 0, z: 0 });
    let center = [15, 5, 5];
    for p in [
        center,
        [14, 5, 5],
        [15, 4, 5],
        [15, 6, 5],
        [15, 5, 4],
        [15, 5, 6],
    ] {
        chunk.blocks.set(Chunk::index(p).unwrap(), LEAVES);
    }
    let mut neighbor = empty_chunk(ChunkKey { x: 0, y: 0, z: 0 });
    neighbor
        .blocks
        .set(Chunk::index([0, 5, 5]).unwrap(), LEAVES);
    let known = HashMap::from([(neighbor.key, Arc::new(neighbor))]);
    let catalog = crate::content::catalog();
    let missing = super::super::mesh_chunk_with_catalog(&chunk, None, 0, catalog, &HashMap::new());
    let loaded = super::super::mesh_chunk_with_catalog(&chunk, None, 0, catalog, &known);
    let belongs_to_center =
        |v: &[f32]| v[0].floor() == -1.0 && v[1].floor() == 5.0 && v[2].floor() == 5.0;
    assert_eq!(
        missing
            .cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .filter(|v| belongs_to_center(v))
            .count(),
        12
    );
    assert_eq!(
        loaded
            .cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .filter(|v| belongs_to_center(v))
            .count(),
        0
    );
    // Boundary clusters that remain exposed keep identical world-hashed geometry.
    let other = |mesh: &crate::render::ChunkMesh| {
        mesh.cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .filter(|v| !belongs_to_center(v))
            .flat_map(|v| v.to_vec())
            .collect::<Vec<_>>()
    };
    assert_eq!(other(&missing), other(&loaded));
}

#[test]
fn buried_leaf_over_a_log_keeps_dense_cap_cards_without_filling_other_interiors() {
    use crate::world::{Chunk, ChunkKey, LEAVES, WOOD};
    let mut chunk = empty_chunk(ChunkKey { x: 0, y: 0, z: 0 });
    for y in 2..5 {
        for z in 2..5 {
            for x in 2..5 {
                chunk.blocks.set(Chunk::index([x, y, z]).unwrap(), LEAVES);
            }
        }
    }
    chunk.blocks.set(Chunk::index([3, 2, 3]).unwrap(), WOOD);
    let mesh = crate::render::mesh::mesh_chunk(&chunk);
    let cards = mesh
        .cutout_vertices
        .chunks_exact(4 * VERTEX_FLOATS)
        .filter(|quad| quad[0].floor() == 3.0 && quad[1].floor() == 3.0 && quad[2].floor() == 3.0)
        .collect::<Vec<_>>();
    assert_eq!(
        cards.len(),
        5,
        "otherwise-buried real leaves above a log must still render"
    );
    let caps = cards
        .iter()
        .filter(|quad| quad[4] > 0.98)
        .collect::<Vec<_>>();
    assert_eq!(
        caps.len(),
        2,
        "two slanted source-alpha sheets cover the cap without inventing opaque art"
    );
    for cap in caps {
        let positions = cap
            .chunks_exact(VERTEX_FLOATS)
            .map(|v| Vec3::from_slice(&v[..3]))
            .collect::<Vec<_>>();
        let min = positions.iter().copied().reduce(Vec3::min).unwrap();
        let max = positions.iter().copied().reduce(Vec3::max).unwrap();
        assert!(
            min.y > 3.0 && max.y < 3.30,
            "cap fill must sit just above the log end"
        );
        assert!(
            max.x - min.x > 0.70 && max.z - min.z > 0.70,
            "cap cards retain broad full-UV coverage"
        );
    }
    let no_support = leaf_cluster([3.0, 3.0, 3.0], 0).count();
    assert_eq!(
        no_support, 0,
        "buried foliage away from timber remains culled"
    );
    assert_eq!(
        mesh.trace.triangles.len(),
        mesh.indices.len() / 3 + mesh.cutout_indices.len() / 3
    );
}
#[test]
#[ignore = "CPU-only foliage mesh budget/timing probe"]
fn generated_grove_mesh_budget_probe() {
    use crate::world::{CHUNK_SIZE, ChunkKey};
    use std::{collections::HashMap, sync::Arc, time::Instant};
    let catalog = crate::content::catalog();
    let mut known = HashMap::new();
    for z in -130..=-127 {
        for x in -46..=-43 {
            for y in 1..=4 {
                let key = ChunkKey { x, y, z };
                known.insert(key, Arc::new(crate::world::generate_chunk(key, 0xB10C6100)));
            }
        }
    }
    let mut old_triangles = 0;
    let mut new_triangles = 0;
    let mut elapsed = std::time::Duration::ZERO;
    let mut bytes = 0;
    for chunk in known.values() {
        let resolved = ResolvedChunk::new(chunk, catalog).unwrap();
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let p = [x, y, z];
                    let block = resolved.block_at(p, CHUNK_SIZE);
                    if !block.botanical || block.has(content::PLANT) || !block.has(content::CUTOUT)
                    {
                        continue;
                    }
                    for axis in 0..3 {
                        for side in [-1, 1] {
                            let mut q = p;
                            let adjacent = p[axis] as i32 + side;
                            let visible = if (0..CHUNK_SIZE as i32).contains(&adjacent) {
                                q[axis] = adjacent as usize;
                                let neighbor = resolved.block_at(q, CHUNK_SIZE);
                                neighbor.id != block.id && !neighbor.has(content::OPAQUE)
                            } else {
                                true
                            };
                            old_triangles += if visible { 2 } else { 0 };
                        }
                    }
                }
            }
        }
        let start = Instant::now();
        let mesh = super::super::mesh_chunk_with_catalog(chunk, None, 0, catalog, &known);
        elapsed += start.elapsed();
        bytes += mesh.draw_byte_len();
        new_triangles += mesh
            .cutout_vertices
            .chunks_exact(4 * VERTEX_FLOATS)
            .filter(|quad| quad[4].abs() < 0.9999)
            .count()
            * 2;
    }
    println!(
        "grove64chunks leaf triangles old_shell={old_triangles} clusters={new_triangles}; mesh_cpu={elapsed:?}; draw_bytes={bytes}"
    );
    assert!(old_triangles > 0 && new_triangles > 0);
}
