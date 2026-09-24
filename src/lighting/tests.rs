use super::*;

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
                        blocks: vec![STONE; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE],
                    }),
                );
            }
        }
    }
    known
}

#[test]
fn sealed_cave_is_dark_and_a_lamp_propagates() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    let room = Arc::make_mut(known.get_mut(&key).unwrap());
    for y in 5..=7 {
        for z in 5..=9 {
            for x in 5..=9 {
                room.blocks[Chunk::index([x, y, z]).unwrap()] = AIR;
            }
        }
    }
    let dark = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(dark.face([7, 4, 7], 1, 1), LightSample::default());

    Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([7, 6, 7]).unwrap()] =
        GLOWSTONE;
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
                room.blocks[Chunk::index([x, y, z]).unwrap()] = AIR;
            }
        }
    }
    room.blocks[Chunk::index([7, 6, 7]).unwrap()] = GLOWSTONE;
    room.blocks[Chunk::index([7, 6, 10]).unwrap()] = MOSS;
    let default = LightField::build(key, &known, 0xB10C_6100);
    let bounced = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    let face = [7, 6, 9];
    assert_eq!(default.face(face, 2, 0).bounce, [0; 3]);
    let reflected = bounced.face(face, 2, 0).bounce;
    Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([7, 6, 10]).unwrap()] = STONE;
    let stone = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert!(reflected[1] > stone.face(face, 2, 0).bounce[1]);
    Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([7, 6, 7]).unwrap()] = AIR;
    let dark = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert_eq!(dark.face(face, 2, 0).bounce, [0; 3]);
}

#[test]
fn opening_a_roof_shaft_relights_the_cave() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let mut known = sealed_neighborhood(key);
    for y in 5..=15 {
        Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([8, y, 8]).unwrap()] = AIR;
    }
    let above = key_offset(key, 0, 1, 0).unwrap();
    for y in 0..CHUNK_SIZE {
        Arc::make_mut(known.get_mut(&above).unwrap()).blocks[Chunk::index([8, y, 8]).unwrap()] =
            AIR;
    }
    let field = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(field.face([8, 5, 8], 1, 1).sky, 15);
    Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([8, 8, 8]).unwrap()] = STONE;
    let closed = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(closed.face([8, 5, 8], 1, 1).sky, 0);
}

#[test]
fn emitted_light_crosses_chunk_seams_and_removal_darkens_both_sides() {
    let key = ChunkKey { x: 0, y: 1, z: 0 };
    let west = key_offset(key, -1, 0, 0).unwrap();
    let mut known = sealed_neighborhood(key);
    Arc::make_mut(known.get_mut(&west).unwrap()).blocks[Chunk::index([15, 6, 8]).unwrap()] =
        GLOWSTONE;
    for x in 0..=3 {
        Arc::make_mut(known.get_mut(&key).unwrap()).blocks[Chunk::index([x, 6, 8]).unwrap()] = AIR;
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
    Arc::make_mut(known.get_mut(&west).unwrap()).blocks[Chunk::index([15, 6, 8]).unwrap()] = STONE;
    let dark = LightField::build(key, &known, 0xB10C_6100);
    assert_eq!(dark.face([2, 5, 8], 1, 1).glow, 0);
    let bounced_dark = LightField::build_with_bounce(key, &known, 0xB10C_6100, true);
    assert_eq!(bounced_dark.face([2, 5, 8], 1, 1).bounce, [0; 3]);
}
