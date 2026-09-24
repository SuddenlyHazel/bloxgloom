use glam::Vec3;

use crate::world::{
    CHUNK_SIZE, Chunk, ChunkKey, DIRT, GLOWSTONE, GRASS, GRAVEL, MOSS, SAND, SNOW, STONE,
};

use super::*;

#[test]
fn solid_chunk_merges_to_six_quads() {
    let chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![1; 16 * 16 * 16],
    };
    assert_eq!(mesh_chunk(&chunk).triangles(), 12);
}

#[test]
fn adjacent_blocks_have_no_internal_faces() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![0; 16 * 16 * 16],
    };
    chunk.blocks[0] = 1;
    chunk.blocks[1] = 1;
    assert_eq!(mesh_chunk(&chunk).triangles(), 12);
}

#[test]
fn meshing_uses_shared_chunk_layout_and_world_origin() {
    let mut chunk = Chunk {
        key: ChunkKey { x: -1, y: 2, z: 3 },
        version: 7,
        blocks: vec![0; 16 * 16 * 16],
    };
    chunk.blocks[Chunk::index([2, 3, 4]).unwrap()] = 3;
    let mesh = mesh_chunk(&chunk);
    let positions = mesh
        .vertices
        .chunks_exact(VERTEX_FLOATS)
        .map(|vertex| &vertex[..3]);
    let (min, max) = positions.fold(
        ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
        |(mut min, mut max), position| {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
            (min, max)
        },
    );
    assert_eq!(min, [-14.0, 35.0, 52.0]);
    assert_eq!(max, [-13.0, 36.0, 53.0]);
    assert_eq!(mesh.version, 7);
}

#[test]
fn grass_uses_top_side_and_underlying_dirt_tiles() {
    assert_eq!(material::material_layer(GRASS, 1, 1), 0);
    assert_eq!(material::material_layer(GRASS, 0, 1), 1);
    assert_eq!(material::material_layer(GRASS, 2, -1), 1);
    assert_eq!(material::material_layer(GRASS, 1, -1), 2);
    assert_eq!(material::material_layer(DIRT, 1, 1), 2);
    assert_eq!(material::material_layer(STONE, 1, 1), 3);
    assert_eq!(material::material_layer(SAND, 1, 1), 4);
    assert_eq!(material::material_layer(SNOW, 1, 1), 5);
    assert_eq!(material::material_layer(MOSS, 1, 1), 6);
    assert_eq!(material::material_layer(GRAVEL, 1, 1), 7);
    assert_eq!(material::material_layer(GLOWSTONE, 1, 1), 8);
}

#[test]
fn grass_side_is_upright_on_both_wall_axes() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![0; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
    };
    chunk.blocks[Chunk::index([1, 1, 1]).unwrap()] = GRASS;
    let mesh = mesh_chunk(&chunk);
    for wall_axis in [0, 2] {
        let vertices = mesh
            .vertices
            .chunks_exact(VERTEX_FLOATS)
            .filter(|vertex| vertex[3 + wall_axis].abs() == 1.0 && vertex[8] == 1.0);
        let mut count = 0;
        for vertex in vertices {
            let expected_v = if vertex[1] == 2.0 { 0.0 } else { 1.0 };
            assert_eq!(vertex[7], expected_v, "grass cap must face world up");
            count += 1;
        }
        assert_eq!(count, 8);
    }
}

#[test]
fn greedy_quads_repeat_material_once_per_voxel() {
    let chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
    };
    let mesh = mesh_chunk(&chunk);
    let vertices = mesh
        .vertices
        .chunks_exact(VERTEX_FLOATS)
        .collect::<Vec<_>>();
    assert_eq!(vertices.len(), 24);
    assert!(vertices.iter().all(|vertex| vertex[8] == 3.0));
    assert!(vertices.iter().any(|vertex| vertex[6] == 16.0));
    assert!(vertices.iter().any(|vertex| vertex[7] == 16.0));
}

#[test]
fn material_mips_are_complete_and_opaque() {
    let mips = material::material_mips();
    assert_eq!(mips.len(), material::TEXTURE_MIPS as usize);
    for (level, pixels) in mips.iter().enumerate() {
        let size = material::TEXTURE_SIZE >> level;
        assert_eq!(
            pixels.len(),
            (size * size * material::TEXTURE_LAYERS * 4) as usize
        );
        assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
    }
    assert_ne!(
        &mips[0][..3],
        &mips[0][(material::TEXTURE_SIZE * material::TEXTURE_SIZE * 3 * 4) as usize..][..3]
    );
}

#[test]
fn material_edges_tile_without_seams() {
    let tiles = material::material_tiles();
    let size = material::TEXTURE_SIZE as usize;
    let layer_bytes = size * size * 4;
    for layer in 0..material::TEXTURE_LAYERS as usize {
        let pixels = &tiles[layer * layer_bytes..(layer + 1) * layer_bytes];
        for y in 0..size {
            let left = &pixels[y * size * 4..y * size * 4 + 3];
            let right = &pixels[(y * size + size - 1) * 4..][..3];
            assert_eq!(left, right, "horizontal seam in layer {layer}, row {y}");
        }
        if layer != 1 {
            for x in 0..size {
                let top = &pixels[x * 4..x * 4 + 3];
                let bottom = &pixels[((size - 1) * size + x) * 4..][..3];
                assert_eq!(top, bottom, "vertical seam in layer {layer}, column {x}");
            }
        }
    }
}

#[test]
fn sky_basis_tracks_camera_turns_in_world_space() {
    let sun = SUN_DIRECTION.normalize();
    let facing = Camera {
        position: Vec3::ZERO,
        yaw: sun.z.atan2(sun.x),
        pitch: sun.y.asin(),
        fov_y_radians: 70.0f32.to_radians(),
    };
    let facing_data = sky_camera_data(facing, 1280, 720);
    let facing_center = Vec3::new(facing_data[0], facing_data[1], facing_data[2]);
    assert!(facing_center.dot(sun) > 0.999);

    let away = Camera {
        yaw: (-sun.z).atan2(-sun.x),
        pitch: -sun.y.asin(),
        ..facing
    };
    let away_data = sky_camera_data(away, 1280, 720);
    let away_center = Vec3::new(away_data[0], away_data[1], away_data[2]);
    assert!(away_center.dot(sun) < -0.999);
    assert!((facing_data[7] - facing_data[11] * (1280.0 / 720.0)).abs() < 1e-6);
}
