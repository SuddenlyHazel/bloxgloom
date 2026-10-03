use super::*;
use crate::world::{AIR, GLOWSTONE, LEAVES, MOSS, RED_FLOWER, WOOD};

#[test]
fn natural_cavern_skylight_crosses_the_zero_height_chunk_boundary() {
    let seed = 0xB10C_6100;
    let key = ChunkKey {
        x: 16,
        y: -1,
        z: 20,
    };
    let mut known = HashMap::new();
    for y in -2..=world::MAX_GENERATED_HEIGHT / CHUNK_SIZE as i32 {
        for z in 19..=21 {
            for x in 15..=17 {
                let key = ChunkKey { x, y, z };
                known.insert(key, Arc::new(world::generate_chunk(key, seed)));
            }
        }
    }
    let field = LightField::build(key, &known, seed);
    let local: HashMap<_, _> = known
        .iter()
        .filter(|(k, _)| k.y <= key.y + 1)
        .map(|(k, chunk)| (*k, Arc::clone(chunk)))
        .collect();
    let fallback = LightField::build(key, &local, seed);
    let mut open_columns = 0;
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = key.x * CHUNK_SIZE as i32 + x as i32;
            let wz = key.z * CHUNK_SIZE as i32 + z as i32;
            let open = (-1..=world::MAX_GENERATED_HEIGHT).all(|y| {
                let (chunk, local) = world::world_to_chunk(wx, y, wz);
                content::catalog().sky_attenuation(known[&chunk].block(local).unwrap()) == 0
            });
            if open {
                open_columns += 1;
                assert_eq!(fallback.face([x, 15, z], 1, 0).sky, 15);
                assert_eq!(
                    field.face([x, 15, z], 1, 0).sky,
                    15,
                    "open natural column {wx}, {wz}"
                );
            }
        }
    }
    assert!(
        open_columns > 0,
        "fixture must contain a naturally open shaft"
    );
}

fn sealed_neighborhood(key: ChunkKey) -> HashMap<ChunkKey, Arc<Chunk>> {
    let mut known = HashMap::new();
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let neighbor = key_offset(key, dx, dy, dz).unwrap();
                known.insert(
                    neighbor,
                    Arc::new(Chunk {
                        key: neighbor,
                        version: 0,
                        blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE].into(),
                    }),
                );
            }
        }
    }
    known
}

#[test]
fn distant_streamed_roof_blocks_and_reopens_a_deep_shaft() {
    let key = ChunkKey { x: 0, y: -2, z: 0 };
    let mut known = sealed_neighborhood(key);
    for y in -2..=5 {
        let upper = ChunkKey { y, ..key };
        let mut chunk = Chunk {
            key: upper,
            version: 1,
            blocks: vec![STONE; world::CHUNK_VOLUME].into(),
        };
        for y in 0..CHUNK_SIZE {
            chunk.blocks.set(Chunk::index([8, y, 8]).unwrap(), AIR);
        }
        known.insert(upper, Arc::new(chunk));
    }
    let roof = ChunkKey { y: 3, ..key };
    for bounced in [false, true] {
        let open = LightField::build_with_bounce(key, &known, 0xB10C_6100, bounced);
        assert_eq!(open.face([8, 8, 8], 1, 0).sky, 15);
        Arc::make_mut(known.get_mut(&roof).unwrap())
            .blocks
            .set(Chunk::index([8, 0, 8]).unwrap(), STONE);
        let closed = LightField::build_with_bounce(key, &known, 0xB10C_6100, bounced);
        assert_eq!(closed.face([8, 8, 8], 1, 0), LightSample::default());
        Arc::make_mut(known.get_mut(&roof).unwrap())
            .blocks
            .set(Chunk::index([8, 0, 8]).unwrap(), AIR);
        let reopened = LightField::build_with_bounce(key, &known, 0xB10C_6100, bounced);
        assert_eq!(reopened.face([8, 8, 8], 1, 0).sky, 15);
    }
}

#[test]
fn sealed_cave_is_dark_and_a_lamp_propagates() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    for y in 5..=7 {
        for z in 5..=9 {
            for x in 5..=9 {
                room.blocks.set(Chunk::index([x, y, z]).unwrap(), AIR);
            }
        }
    }
    let dark = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(dark.face([7, 4, 7], 1, 1), LightSample::default());

    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([7, 6, 7]).unwrap(), GLOWSTONE);
    let lit = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(lit.face([7, 4, 7], 1, 1).glow, 14);
    assert_eq!(lit.face([7, 4, 7], 1, 1).sky, 0);
}

#[test]
fn bounced_mode_reflects_surface_color_without_leaking_into_default() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    for y in 5..=7 {
        for z in 5..=9 {
            for x in 5..=9 {
                room.blocks.set(Chunk::index([x, y, z]).unwrap(), AIR);
            }
        }
    }
    room.blocks.set(Chunk::index([7, 6, 7]).unwrap(), GLOWSTONE);
    room.blocks.set(Chunk::index([7, 6, 10]).unwrap(), MOSS);
    let default = LightField::build(key, &known, 0xB10C_6100);
    let bounced = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    let face = [7, 6, 9];
    assert_eq!(default.face(face, 2, 0).bounce, [0; 3]);
    let reflected = bounced.face(face, 2, 0).bounce;
    assert_ne!(reflected, [0; 3]);
    // Sealed rooms have no sunlight: their reflections must survive at night.
    assert_eq!(bounced.face(face, 2, 0).glow_bounce, reflected);
    assert_eq!(default.face(face, 2, 0).glow_bounce, [0; 3]);
    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([7, 6, 10]).unwrap(), STONE);
    let stone = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert!(reflected[1] > stone.face(face, 2, 0).bounce[1]);
    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([7, 6, 7]).unwrap(), AIR);
    let dark = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert_eq!(dark.face(face, 2, 0).bounce, [0; 3]);
    assert_eq!(dark.face(face, 2, 0).glow_bounce, [0; 3]);
}

#[test]
fn opening_a_roof_shaft_relights_the_cave() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    for y in 5..=15 {
        Arc::make_mut(known.get_mut(&key).unwrap())
            .blocks
            .set(Chunk::index([8, y, 8]).unwrap(), AIR);
    }
    let above = key_offset(key, 0, 1, 0).unwrap();
    for y in 0..CHUNK_SIZE {
        Arc::make_mut(known.get_mut(&above).unwrap())
            .blocks
            .set(Chunk::index([8, y, 8]).unwrap(), AIR);
    }
    let field = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(field.face([8, 5, 8], 1, 1).sky, 15);
    let reflected = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    let daylight = reflected.face([8, 5, 8], 1, 1);
    assert_ne!(daylight.bounce, [0; 3]);
    assert_eq!(daylight.glow_bounce, [0; 3]);
    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([8, 8, 8]).unwrap(), STONE);
    let closed = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(closed.face([8, 5, 8], 1, 1).sky, 0);
}

#[test]
fn emitted_light_crosses_chunk_seams_and_removal_darkens_both_sides() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let west = key_offset(key, -1, 0, 0).unwrap();
    let mut known = sealed_neighborhood(key);
    Arc::make_mut(known.get_mut(&west).unwrap())
        .blocks
        .set(Chunk::index([15, 6, 8]).unwrap(), GLOWSTONE);
    for x in 0..=3 {
        Arc::make_mut(known.get_mut(&key).unwrap())
            .blocks
            .set(Chunk::index([x, 6, 8]).unwrap(), AIR);
    }
    let lit = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(lit.face([2, 5, 8], 1, 1).glow, 12);
    let bounced = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert!(
        bounced
            .face([2, 5, 8], 1, 1)
            .bounce
            .iter()
            .any(|&channel| channel > 0)
    );
    Arc::make_mut(known.get_mut(&west).unwrap())
        .blocks
        .set(Chunk::index([15, 6, 8]).unwrap(), STONE);
    let dark = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(dark.face([2, 5, 8], 1, 1).glow, 0);
    let bounced_dark = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert_eq!(bounced_dark.face([2, 5, 8], 1, 1).bounce, [0; 3]);
}

#[test]
fn plants_transmit_and_leaves_partially_attenuate_daylight() {
    let key = ChunkKey { x: 0, y: 2, z: 0 };
    let mut known = sealed_neighborhood(key);
    for y in 0..CHUNK_SIZE {
        let above = key_offset(key, 0, 1, 0).unwrap();
        Arc::make_mut(known.get_mut(&above).unwrap())
            .blocks
            .set(Chunk::index([8, y, 8]).unwrap(), AIR);
        Arc::make_mut(known.get_mut(&key).unwrap())
            .blocks
            .set(Chunk::index([8, y, 8]).unwrap(), AIR);
    }
    let center = Arc::make_mut(known.get_mut(&key).unwrap());
    center.blocks.set(Chunk::index([8, 11, 8]).unwrap(), LEAVES);
    center
        .blocks
        .set(Chunk::index([8, 6, 8]).unwrap(), RED_FLOWER);
    let open = LightField::build(key, &known, 7);
    assert_eq!(open.face([8, 4, 8], 1, 1).sky, 13);
    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([8, 11, 8]).unwrap(), WOOD);
    let closed = LightField::build(key, &known, 7);
    assert_eq!(closed.face([8, 4, 8], 1, 1).sky, 0);
}

#[test]
fn mapped_glowstone_definition_supplies_emission_to_light_builder() {
    use crate::content::ContentManifest;

    let local = crate::content::Catalog::builtins();
    let mut manifest = ContentManifest::from_catalog(&local);
    for entry in &mut manifest.entries {
        match (entry.kind, entry.key.as_str()) {
            (b'B', "bloxgloom:glowstone") => entry.id = 65_536,
            (b'S', "bloxgloom:glowstone") => entry.id = 65_537,
            (b'I', "bloxgloom:glowstone") => entry.id = 65_538,
            _ => {}
        }
    }
    manifest
        .entries
        .sort_unstable_by_key(|entry| (entry.kind, entry.id));
    let catalog = manifest.resolve_catalog(&local).unwrap();
    let mapped_glowstone = BlockId::new(65_537);
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    for y in 5..=7 {
        for z in 5..=9 {
            for x in 5..=9 {
                room.blocks.set(Chunk::index([x, y, z]).unwrap(), AIR);
            }
        }
    }
    room.blocks
        .set(Chunk::index([7, 6, 7]).unwrap(), mapped_glowstone);

    let light = LightField::build_with_catalog(key, &known, 0xB10C_6100, &catalog);
    assert_eq!(catalog.emission(mapped_glowstone), 15);
    assert_eq!(light.face([7, 6, 6], 2, 1).glow, 15);
    assert_eq!(light.face([7, 6, 5], 2, 1).glow, 14);
}

#[test]
fn layered_canopy_absorbs_sky_above_the_local_halo_and_relights_after_edit() {
    let key = ChunkKey { x: 0, y: 2, z: 0 };
    let mut known = sealed_neighborhood(key);
    // A sealed shaft isolates direct transmission from light entering its sides.
    for cy in 1..=4 {
        let chunk_key = ChunkKey { y: cy, ..key };
        let mut chunk = Chunk {
            key: chunk_key,
            version: 0,
            blocks: vec![STONE; world::CHUNK_VOLUME].into(),
        };
        for y in 0..CHUNK_SIZE {
            chunk.blocks.set(Chunk::index([8, y, 8]).unwrap(), AIR);
        }
        known.insert(chunk_key, Arc::new(chunk));
    }
    let roof = ChunkKey { y: 4, ..key };
    for y in [4, 5, 6] {
        Arc::make_mut(known.get_mut(&roof).unwrap())
            .blocks
            .set(Chunk::index([8, y, 8]).unwrap(), LEAVES);
    }
    let shaded = LightField::build(key, &known, 7);
    assert_eq!(shaded.face([8, 4, 8], 1, 0).sky, 9);
    for y in [4, 5, 6] {
        Arc::make_mut(known.get_mut(&roof).unwrap())
            .blocks
            .set(Chunk::index([8, y, 8]).unwrap(), AIR);
    }
    let reopened = LightField::build(key, &known, 7);
    assert_eq!(reopened.face([8, 4, 8], 1, 0).sky, 15);
}

fn colored_lamp_catalog() -> (Catalog, BlockId, BlockId) {
    let mut catalog = Catalog::builtins();
    let mut ids = Vec::new();
    for (id, color) in [(10000, [255, 0, 0]), (10001, [0, 0, 255])] {
        let mut block = catalog.block(GLOWSTONE).unwrap().clone();
        block.id = content::BlockTypeId(id);
        block.key = format!("test:lamp_{id}").into();
        block.reflectance = color;
        catalog.register_block(block).unwrap();
        let state = content::BlockStateId(id);
        catalog
            .register_state(state, content::BlockTypeId(id), vec![], None)
            .unwrap();
        ids.push(state);
    }
    (catalog, ids[0], ids[1])
}

#[test]
fn colored_transport_follows_bent_passage_and_edit_closes_it() {
    let (catalog, red, _) = colored_lamp_catalog();
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    for p in [
        [4, 6, 4],
        [5, 6, 4],
        [6, 6, 4],
        [6, 6, 5],
        [6, 6, 6],
        [6, 6, 7],
    ] {
        room.blocks.set(Chunk::index(p).unwrap(), AIR);
    }
    room.blocks.set(Chunk::index([4, 6, 4]).unwrap(), red);
    let field = LightField::build_with_bounce_and_catalog(key, &known, 1, true, &catalog);
    let sample = field.face([6, 6, 7], 0, 0);
    assert_eq!(sample.glow_color, [255, 0, 0]);
    assert_eq!(
        sample.glow_direction,
        [0, 0, -127],
        "light arrives down passage, not diagonally through wall"
    );
    assert_eq!(sample.sky, 0);
    assert!(sample.glow_bounce[0] > 0);
    assert_eq!(&sample.glow_bounce[1..], &[0, 0]);
    let (rgb, direction) = field.spatial_local([6.5, 6.5, 7.5]);
    assert_eq!(rgb, [f32::from(sample.glow) / 15.0, 0.0, 0.0]);
    assert_eq!(direction, [0.0, 0.0, -1.0]);
    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([6, 6, 5]).unwrap(), STONE);
    let closed = LightField::build_with_bounce_and_catalog(key, &known, 1, true, &catalog);
    assert_eq!(closed.face([6, 6, 7], 0, 0), LightSample::default());
}

#[test]
fn colored_transport_ties_are_stable_across_chunk_views_and_insertion_order() {
    let (catalog, red, blue) = colored_lamp_catalog();
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let east = key_offset(key, 1, 0, 0).unwrap();
    let mut known = sealed_neighborhood(key);
    for x in 12..=20 {
        let (k, local) = world::world_to_chunk(x, key.y * CHUNK_SIZE as i32 + 6, 8);
        Arc::make_mut(known.get_mut(&k).unwrap())
            .blocks
            .set(Chunk::index(local).unwrap(), AIR);
    }
    Arc::make_mut(known.get_mut(&key).unwrap())
        .blocks
        .set(Chunk::index([12, 6, 8]).unwrap(), red);
    Arc::make_mut(known.get_mut(&east).unwrap())
        .blocks
        .set(Chunk::index([4, 6, 8]).unwrap(), blue);
    let left = LightField::build_with_catalog(key, &known, 1, &catalog);
    let mut entries: Vec<_> = known.iter().collect();
    entries.reverse();
    let reordered = entries
        .into_iter()
        .map(|(k, v)| (*k, Arc::clone(v)))
        .collect();
    let right = LightField::build_with_catalog(east, &reordered, 1, &catalog);
    let a = left.face([16, 6, 8], 0, 0);
    let b = right.face([0, 6, 8], 0, 0);
    assert_eq!(a, b);
    assert_eq!(
        a.glow_color,
        [255, 0, 0],
        "equal levels choose stable RGB priority"
    );
    assert_eq!(a.glow_direction, [-127, 0, 0]);
    assert_eq!(
        left.spatial_local([16.0, 6.5, 8.5]),
        right.spatial_local([0.0, 6.5, 8.5])
    );
    assert_eq!(
        left.corner_local([1, 0, 2], 1, 5, [16, 8]),
        right.corner_local([1, 0, 2], 1, 5, [0, 8])
    );
}

#[test]
fn opaque_emitter_keeps_own_tint_beside_brighter_emitter() {
    let (mut catalog, red, blue) = colored_lamp_catalog();
    let mut dim = catalog.block(blue).unwrap().clone();
    dim.id = content::BlockTypeId(10002);
    dim.key = "test:dim_blue".into();
    dim.emission = 14;
    catalog.register_block(dim).unwrap();
    let dim_blue = content::BlockStateId(10002);
    catalog
        .register_state(dim_blue, content::BlockTypeId(10002), vec![], None)
        .unwrap();
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    room.blocks.set(Chunk::index([5, 6, 8]).unwrap(), red);
    room.blocks.set(Chunk::index([6, 6, 8]).unwrap(), dim_blue);
    room.blocks.set(Chunk::index([7, 6, 8]).unwrap(), AIR);
    let field = LightField::build_with_catalog(key, &known, 1, &catalog);
    assert_eq!(field.face([6, 6, 8], 0, 0).glow_color, [0, 0, 255]);
    assert_eq!(field.face([7, 6, 8], 0, 0).glow_color, [0, 0, 255]);
    assert_eq!(field.face([7, 6, 8], 0, 0).glow, 13);
}
