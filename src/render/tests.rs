use glam::Vec3;

pub(super) mod studio;

use crate::items::{SAPLING, SEEDS, STICK};
use crate::world::{
    AIR, CHUNK_SIZE, Chunk, ChunkKey, DIRT, FERN, GLOWSTONE, GRASS, GRAVEL, LEAVES, MOSS,
    RED_FLOWER, SAND, SNOW, STONE, TALL_GRASS, WOOD, WOOD_X, WOOD_Z,
};

use super::*;

#[test]
fn public_fixture_assets_reach_material_upload_lighting_and_mesh_compilation() {
    use bloxgloom_lifecycle_fixture::content::{Content, LAMP, REED, TEXTURE};
    let catalog =
        crate::server::catalog_with_extension(crate::content::Catalog::builtins(), &Content)
            .unwrap();
    let lamp = catalog
        .state_by_key(&format!("{LAMP}[axis=x,lit=true]"))
        .unwrap();
    let reed = catalog.state_by_key(REED).unwrap();
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 10, z: 0 },
        version: 1,
        blocks: vec![AIR; CHUNK_SIZE.pow(3)].into(),
    };
    chunk.blocks.set(Chunk::index([3, 2, 4]).unwrap(), lamp);
    chunk.blocks.set(Chunk::index([3, 3, 4]).unwrap(), reed);
    let known = std::collections::HashMap::from([(chunk.key, Arc::new(chunk.clone()))]);
    let light = crate::lighting::LightField::build_with_catalog(chunk.key, &known, 7, &catalog);
    let mesh = mesh::mesh_chunk_lit_with_catalog(&chunk, &light, 1, &catalog);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.cutout_indices.len(), 12);
    // The face samples the adjacent cell: normal propagation attenuates 13 to 12.
    assert_eq!(light.face([3, 2, 4], 0, 1).glow, 12);
    let layer = catalog
        .textures()
        .iter()
        .position(|t| t.key == TEXTURE)
        .unwrap();
    let emission = material::emission_strengths(&catalog);
    assert_eq!(emission[layer], 1.25);
    assert_eq!(
        emission[material::material_layer_for(&catalog, GLOWSTONE, 1, 1) as usize],
        3.5
    );
    assert_eq!(
        emission[material::material_layer_for(&catalog, STONE, 1, 1) as usize],
        0.0
    );
    assert!(
        mesh.cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .all(|v| v[8].floor() == layer as f32)
    );
    for vertices in [&mesh.vertices, &mesh.cutout_vertices] {
        assert_eq!(vertices.len() % VERTEX_FLOATS, 0);
        assert!(
            vertices.chunks_exact(VERTEX_FLOATS).any(|v| v[13] > 0.0),
            "catalog source color must reach cube and plant vertices"
        );
        assert!(
            vertices.chunks_exact(VERTEX_FLOATS).any(|v| v[14] > 0.0),
            "occluded propagation direction must reach cube and plant vertices"
        );
    }
    let tiles = material::material_tiles_for(&catalog);
    let stride = (material::TEXTURE_SIZE.pow(2) * 4) as usize;
    let packed = material::layers::Layers::new(&catalog).by_texture[layer] as usize;
    let pixels = &tiles[packed * stride..(packed + 1) * stride];
    assert!(pixels.chunks_exact(4).any(|p| p[3] == 0));
    assert!(pixels.chunks_exact(4).any(|p| p[3] == 255));
}

#[test]
fn urgent_mesh_reorders_existing_pending_chunk_without_duplication() {
    let a = ChunkKey { x: 0, y: 0, z: 0 };
    let b = ChunkKey { x: 1, y: 0, z: 0 };
    let c = ChunkKey { x: 2, y: 0, z: 0 };
    let mut order = VecDeque::from([a, b, c]);
    order_pending_mesh(&mut order, b, true, true);
    assert_eq!(order, VecDeque::from([b, a, c]));
    order_pending_mesh(&mut order, b, true, true);
    assert_eq!(order, VecDeque::from([b, a, c]));
}

#[test]
fn uploads_leave_background_a_turn_during_continuous_immediate_edits() {
    let keys = (0..5)
        .map(|x| ChunkKey { x, y: 0, z: 0 })
        .collect::<Vec<_>>();
    let immediate = keys[..4].iter().copied().collect();
    let order = keys.iter().copied().collect();
    assert_eq!(next_upload_index(&order, &immediate, 2), 0);
    assert_eq!(next_upload_index(&order, &immediate, 3), 4);
    let all_immediate = keys.iter().copied().collect();
    assert_eq!(next_upload_index(&order, &all_immediate, 3), 0);
}

fn mapped_builtin(
    name: &str,
) -> (
    crate::content::Catalog,
    crate::content::BlockStateId,
    crate::items::ItemId,
) {
    use crate::content::{BlockStateId, ContentManifest, ItemId};

    let local = crate::content::Catalog::builtins();
    let key = format!("bloxgloom:{name}");
    let mut manifest = ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        match (entry.kind, entry.key.as_str()) {
            (b'B', candidate) if candidate == key => entry.id = 65_536,
            (b'S', candidate) if candidate == key => entry.id = 65_537,
            (b'I', candidate) if candidate == key => entry.id = 65_538,
            _ => {}
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    (
        manifest.resolve_catalog(&local).unwrap(),
        BlockStateId::new(65_537),
        ItemId::new(65_538),
    )
}

#[test]
fn solid_chunk_merges_to_six_quads() {
    let chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![GRASS; 16 * 16 * 16].into(),
    };
    assert_eq!(mesh_chunk(&chunk).triangles(), 12);
}

#[test]
fn adjacent_blocks_have_no_internal_faces() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![AIR; 16 * 16 * 16].into(),
    };
    chunk.blocks.set(0, GRASS);
    chunk.blocks.set(1, GRASS);
    assert_eq!(mesh_chunk(&chunk).triangles(), 12);
}

#[test]
fn meshing_uses_shared_chunk_layout_and_world_origin() {
    let mut chunk = Chunk {
        key: ChunkKey { x: -1, y: 2, z: 3 },
        version: 7,
        blocks: vec![AIR; 16 * 16 * 16].into(),
    };
    chunk.blocks.set(Chunk::index([2, 3, 4]).unwrap(), STONE);
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
    assert_eq!(material::material_layer(WOOD, 0, 1), 9);
    assert_eq!(material::material_layer(WOOD, 1, 1), 10);
    assert_eq!(material::material_layer(WOOD_X, 0, 1), 10);
    assert_eq!(material::material_layer(WOOD_X, 1, 1), 9);
    assert_eq!(material::material_layer(WOOD_Z, 2, 1), 10);
    assert_eq!(material::material_layer(WOOD_Z, 1, 1), 9);
    assert_eq!(material::material_layer(LEAVES, 1, 1), 11);
    assert_eq!(material::material_layer(RED_FLOWER, 1, 1), 12);
    assert_eq!(material::material_layer(FERN, 1, 1), 15);
    assert_eq!(material::material_layer(TALL_GRASS, 1, 1), 16);
    assert_eq!(material::item_material_layer(SEEDS, 1, 1), 17);
    assert_eq!(material::item_material_layer(SAPLING, 1, 1), 18);
    assert_eq!(material::item_material_layer(STICK, 1, 1), 19);
}

#[test]
fn grass_side_is_upright_on_both_wall_axes() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![AIR; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into(),
    };
    chunk.blocks.set(Chunk::index([1, 1, 1]).unwrap(), GRASS);
    let mesh = mesh_chunk(&chunk);
    for wall_axis in [0, 2] {
        let vertices = mesh
            .vertices
            .chunks_exact(VERTEX_FLOATS)
            .filter(|vertex| vertex[3 + wall_axis].abs() == 1.0 && vertex[8].floor() == 1.0);
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
        blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into(),
    };
    let mesh = mesh_chunk(&chunk);
    let vertices = mesh
        .vertices
        .chunks_exact(VERTEX_FLOATS)
        .collect::<Vec<_>>();
    assert_eq!(vertices.len(), 24);
    assert!(vertices.iter().all(|vertex| vertex[8].floor() == 3.0));
    assert!(vertices.iter().any(|vertex| vertex[6] == 16.0));
    assert!(vertices.iter().any(|vertex| vertex[7] == 16.0));
}

#[test]
fn material_mips_preserve_opaque_and_cutout_layers() {
    let leaf_layer = material::material_layer(LEAVES, 1, 1) as usize;
    assert!(crate::content::catalog().textures()[leaf_layer].alpha_cutout);
    let mips = material::material_mips();
    assert_eq!(mips.len(), material::TEXTURE_MIPS as usize);
    for (level, pixels) in mips.iter().enumerate() {
        let size = material::TEXTURE_SIZE >> level;
        assert_eq!(
            pixels.len(),
            (size * size * material::texture_layers() * 4) as usize
        );
        let layer_bytes = (size * size * 4) as usize;
        assert!(
            pixels[..11 * layer_bytes]
                .chunks_exact(4)
                .all(|pixel| pixel[3] == 255)
        );
    }
    assert_ne!(
        &mips[0][..3],
        &mips[0][(material::TEXTURE_SIZE * material::TEXTURE_SIZE * 3 * 4) as usize..][..3]
    );
    let layer_bytes = (material::TEXTURE_SIZE * material::TEXTURE_SIZE * 4) as usize;
    let layers = material::layers::Layers::new(crate::content::catalog());
    for (texture_id, texture) in crate::content::catalog()
        .textures()
        .iter()
        .enumerate()
        .filter(|(_, texture)| texture.alpha_cutout)
    {
        let layer = layers.by_texture[texture_id] as usize;
        let mut alpha = mips[0][layer * layer_bytes..(layer + 1) * layer_bytes]
            .chunks_exact(4)
            .map(|pixel| pixel[3]);
        assert!(
            alpha.any(|value| value >= 128),
            "{} (layer {layer}) needs visible texels",
            texture.key
        );
    }
}

#[test]
fn registered_texture_and_block_extend_material_array_without_shader_changes() {
    use crate::content::{
        BlockDef, BlockStateId, BlockTextures, BlockTypeId, Catalog, ItemDef, TextureDef, TextureId,
    };
    use crate::items::ItemId;
    use std::borrow::Cow;

    let mut catalog = Catalog::builtins();
    let layer = catalog
        .register_texture(TextureDef {
            key: "example:marble_tile".into(),
            png: Cow::Borrowed(include_bytes!("../../assets/textures/blocks/stone.png")),
            stitch_edges: true,
            stitch_vertical: true,
            alpha_cutout: false,
            emission_strength: 0.0,
            foliage: Default::default(),
        })
        .unwrap();
    catalog
        .register_block(BlockDef {
            id: BlockTypeId::new(17),
            key: "example:marble".into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.9, 0.9, 1.0],
            textures: BlockTextures {
                top: layer,
                side: layer,
                bottom: layer,
            },
            solid: true,
            opaque: true,
            fluid: false,
            cutout: false,
            plant: false,
            replaceable: false,
            supports_plant: false,
            flammable: false,
            emission: 0,
            sky_attenuation: 0,
            reflectance: [180, 180, 180],
            properties: Vec::new(),
        })
        .unwrap();
    catalog
        .register_state(
            BlockStateId::new(17),
            BlockTypeId::new(17),
            Vec::new(),
            None,
        )
        .unwrap();
    catalog
        .register_item(ItemDef {
            id: ItemId::new(131),
            key: "example:marble".into(),
            name: "MARBLE".into(),
            swatch: [0.9, 0.9, 0.9, 1.0],
            texture: layer,
            placeable: Some(BlockStateId::new(17)),
            sprite: false,
        })
        .unwrap();
    catalog
        .register_item(ItemDef {
            id: ItemId::new(17),
            key: "example:token".into(),
            name: "TOKEN".into(),
            swatch: [0.8, 0.6, 0.2, 1.0],
            texture: TextureId::new(17),
            placeable: None,
            sprite: true,
        })
        .unwrap();
    assert_eq!(
        material::material_layer_for(&catalog, BlockStateId::new(17), 0, 1),
        layer.0
    );
    assert_eq!(
        catalog.item(ItemId::new(131)).unwrap().placeable,
        Some(BlockStateId::new(17))
    );
    assert_eq!(
        material::item_material_layer_for(&catalog, ItemId::new(131), 1, 1),
        layer.0
    );
    assert_eq!(
        material::item_material_layer_for(&catalog, ItemId::new(17), 1, 1),
        17
    );
    assert_eq!(
        material::material_tiles_for(&catalog).len(),
        (material::texture_layers_for(&catalog)
            * material::TEXTURE_SIZE
            * material::TEXTURE_SIZE
            * 4) as usize
    );
}

#[test]
fn remapped_connection_catalog_drives_foliage_meshes_and_drop_art() {
    use crate::lighting::LightField;
    use crate::world::Chunk;

    let (catalog, flower, item) = mapped_builtin("red_flower");
    let key = ChunkKey { x: 0, y: 0, z: 0 };
    let mut chunk = Chunk::from_blocks(key, 3, vec![AIR; CHUNK_SIZE.pow(3)]);
    chunk.blocks.set(Chunk::index([3, 4, 5]).unwrap(), flower);
    let known = std::collections::HashMap::from([(key, std::sync::Arc::new(chunk.clone()))]);
    let light = LightField::build_with_catalog(key, &known, 12, &catalog);
    let mesh = mesh_chunk_lit_with_catalog(&chunk, &light, 8, &catalog);

    assert!(catalog.state(crate::world::RED_FLOWER).is_none());
    assert_eq!(mesh.cutout_indices.len(), 12);
    assert!(
        mesh.cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .all(|vertex| vertex[8].floor() == 12.0)
    );

    let drop = super::drops::mesh_with_catalog(
        &[VisualDrop {
            item,
            center: Vec3::new(4.0, 5.0, 6.0),
            light: Default::default(),
            angle: 0.3,
            scale: 1.0,
        }],
        &catalog,
    );
    assert!(drop.opaque_indices.is_empty());
    assert_eq!(drop.cutout_indices.len(), 12);
    assert!(
        drop.cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .all(|vertex| vertex[8] == 12.0)
    );
}

#[test]
fn material_edges_tile_only_when_requested() {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[
                230, 10, 50, 255, 10, 90, 120, 255, 20, 150, 200, 255, 60, 230, 40, 255,
            ])
            .unwrap();
    }
    let mut catalog = crate::content::Catalog::new();
    for (key, horizontal, vertical) in [
        ("test:both", true, true),
        ("test:horizontal", true, false),
        ("test:authored", false, false),
    ] {
        catalog
            .register_texture(crate::content::TextureDef {
                key: key.into(),
                png: std::borrow::Cow::Owned(encoded.clone()),
                stitch_edges: horizontal,
                stitch_vertical: vertical,
                alpha_cutout: false,
                emission_strength: 0.0,
                foliage: Default::default(),
            })
            .unwrap();
    }
    let tiles = material::material_tiles_for(&catalog);
    let size = material::TEXTURE_SIZE as usize;
    let layer_bytes = size * size * 4;
    for (layer, texture) in catalog.textures().iter().enumerate() {
        let pixels = &tiles[layer * layer_bytes..(layer + 1) * layer_bytes];
        if !texture.stitch_edges {
            assert_eq!(
                &pixels[..4],
                &[230, 10, 50, 255],
                "authored art is preserved"
            );
            continue;
        }
        for y in 0..size {
            let left = &pixels[y * size * 4..y * size * 4 + 3];
            let right = &pixels[(y * size + size - 1) * 4..][..3];
            assert_eq!(left, right, "horizontal seam in {}, row {y}", texture.key);
        }
        if texture.stitch_vertical {
            for x in 0..size {
                let top = &pixels[x * 4..x * 4 + 3];
                let bottom = &pixels[((size - 1) * size + x) * 4..][..3];
                assert_eq!(top, bottom, "vertical seam in {}, column {x}", texture.key);
            }
        } else {
            assert_ne!(
                &pixels[..3],
                &pixels[(size - 1) * size * 4..][..3],
                "directional top/bottom art remains distinct"
            );
        }
    }
}

#[test]
fn plants_have_two_crossed_cutout_quads_and_do_not_hide_ground() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![AIR; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into(),
    };
    chunk.blocks.set(Chunk::index([3, 2, 4]).unwrap(), GRASS);
    chunk
        .blocks
        .set(Chunk::index([3, 3, 4]).unwrap(), RED_FLOWER);
    let mesh = mesh_chunk(&chunk);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.cutout_indices.len(), 12);
    assert_eq!(mesh.cutout_vertices.len(), 8 * VERTEX_FLOATS);
    assert!(
        mesh.cutout_vertices
            .chunks_exact(VERTEX_FLOATS)
            .all(|vertex| vertex[8].floor() == 12.0)
    );
}

#[test]
fn adjacent_leaves_use_bounded_card_clusters() {
    let mut chunk = Chunk {
        key: ChunkKey { x: 0, y: 0, z: 0 },
        version: 0,
        blocks: vec![AIR; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into(),
    };
    chunk.blocks.set(Chunk::index([3, 2, 4]).unwrap(), LEAVES);
    chunk.blocks.set(Chunk::index([4, 2, 4]).unwrap(), LEAVES);
    let mesh = mesh_chunk(&chunk);
    assert!(mesh.indices.is_empty());
    // Both boundary cells retain four crossed cards; their shared face does
    // not add a cube wall or duplicate the cluster.
    assert_eq!(mesh.cutout_indices.len(), 8 * 6);
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
    let facing_data = sky_camera_data(
        facing,
        1280,
        720,
        daylight::Atmosphere::at(crate::daylight::INITIAL_MS),
    );
    let facing_center = Vec3::new(facing_data[0], facing_data[1], facing_data[2]);
    assert!(facing_center.dot(sun) > 0.999);

    let away = Camera {
        yaw: (-sun.z).atan2(-sun.x),
        pitch: -sun.y.asin(),
        ..facing
    };
    let away_data = sky_camera_data(
        away,
        1280,
        720,
        daylight::Atmosphere::at(crate::daylight::INITIAL_MS),
    );
    let away_center = Vec3::new(away_data[0], away_data[1], away_data[2]);
    assert!(away_center.dot(sun) < -0.999);
    assert!((facing_data[7] - facing_data[11] * (1280.0 / 720.0)).abs() < 1e-6);
}

#[test]
fn fire_streak_mesh_is_bounded_and_rises_before_fading() {
    let fire = fire::VisualFire {
        center: Vec3::new(4.5, 9.5, -3.5),
        age: 0.25,
        style: fire::FireStyle::Flame,
    };
    let mesh = fire::vertices(&[fire; fire::MAX_FIRES + 1]);
    assert_eq!(mesh.len() * 4, fire::MAX_BYTES as usize);
    assert!(mesh.chunks_exact(9).all(|vertex| {
        let position = Vec3::new(vertex[0], vertex[1], vertex[2]);
        position.distance(fire.center) < 1.2 && (0.0..=1.0).contains(&vertex[8])
    }));
    let fading = fire::vertices(&[fire::VisualFire { age: 0.9, ..fire }]);
    assert!(fading[8] < fire::vertices(&[fire])[8]);
}

#[test]
fn colored_spark_mesh_uses_its_own_shape_and_fades() {
    let spark = fire::VisualFire {
        center: Vec3::ZERO,
        age: 0.25,
        style: fire::FireStyle::Spark([0.2, 0.8, 1.0], 0.16),
    };
    let vertices = fire::vertices(&[spark]);
    assert_eq!(vertices.len() / 9, 12);
    assert!(vertices.chunks_exact(9).all(|vertex| {
        vertex[5..8] == [0.2, 0.8, 1.0]
            && (0.0..=1.0).contains(&vertex[8])
            && Vec3::new(vertex[0], vertex[1], vertex[2]).length() < 1.0
    }));
    let faded = fire::vertices(&[fire::VisualFire { age: 0.9, ..spark }]);
    assert!(faded[8] < vertices[8]);
    let larger = fire::vertices(&[fire::VisualFire {
        style: fire::FireStyle::Spark([0.2, 0.8, 1.0], 0.28),
        ..spark
    }]);
    assert!(larger[0].abs() > vertices[0].abs());
    assert_eq!(larger.len(), vertices.len());
}
