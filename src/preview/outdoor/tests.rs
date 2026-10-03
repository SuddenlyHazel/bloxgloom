use super::*;

fn chunks() -> HashMap<ChunkKey, Arc<world::Chunk>> {
    let mut chunks = HashMap::new();
    for z in -2..=2 {
        for x in -2..=2 {
            for y in 0..=4 {
                let key = ChunkKey { x, y, z };
                chunks.insert(
                    key,
                    Arc::new(world::Chunk::from_blocks(
                        key,
                        0,
                        vec![world::GLOWSTONE; world::CHUNK_VOLUME],
                    )),
                );
            }
        }
    }
    chunks
}

fn block(chunks: &HashMap<ChunkKey, Arc<world::Chunk>>, xyz: [i32; 3]) -> world::BlockId {
    let (key, local) = world::world_to_chunk(xyz[0], xyz[1], xyz[2]);
    chunks[&key].block(local).unwrap()
}

#[test]
fn authored_scene_replaces_terrain_and_preserves_layered_canopy_and_portal() {
    let mut chunks = chunks();
    prepare(View::Overview, &mut chunks);
    assert_eq!(block(&chunks, [0, 32, 0]), world::GRASS);
    assert_eq!(block(&chunks, [0, 60, 0]), world::AIR);
    for y in 38..=41 {
        assert_eq!(block(&chunks, [-9, y, -12]), world::LEAVES);
    }
    assert_eq!(block(&chunks, [-12, 35, -15]), world::WOOD);
    assert_eq!(block(&chunks, [18, 35, -8]), world::AIR);
    assert_eq!(block(&chunks, [18, 38, -8]), world::STONE);
    assert_eq!(block(&chunks, [18, 35, -29]), world::STONE);
    assert!(!chunks.values().any(|chunk| {
        (0..world::CHUNK_VOLUME).any(|i| chunk.block_index(i) == Some(world::GLOWSTONE))
    }));
    let before: Vec<_> = chunks
        .iter()
        .map(|(key, chunk)| (*key, chunk.clone()))
        .collect();
    prepare(View::Interior, &mut chunks);
    for (key, old) in before {
        for i in 0..world::CHUNK_VOLUME {
            assert_eq!(old.block_index(i), chunks[&key].block_index(i));
        }
    }
}

#[test]
fn light_samples_separate_open_sky_canopy_and_dark_cave() {
    let mut chunks = chunks();
    prepare(View::Overview, &mut chunks);
    let sample = |xyz: [i32; 3]| {
        let (key, local) = world::world_to_chunk(xyz[0], xyz[1], xyz[2]);
        LightField::build_with_bounce(key, &chunks, SEED, false).face(local, 1, 0)
    };
    assert_eq!(sample([2, 34, 0]).sky, 15);
    let canopy = sample([-9, 34, -12]);
    assert!(canopy.sky > 0 && canopy.sky < 15);
    let deep = sample([18, 34, -27]);
    assert_eq!([deep.sky, deep.glow], [0, 0]);
}

#[test]
fn temporal_motion_moves_actors_without_mutating_base_and_cuts_to_correct_eye() {
    let mut chunks = chunks();
    prepare(View::Motion, &mut chunks);
    let base = avatars(&chunks);
    let warm = motion::actors(&base, 7);
    let moving = motion::actors(&base, 8);
    assert_eq!(warm[2].position, base[2].position);
    assert!((moving[2].position.x - base[2].position.x - 0.18).abs() < 1e-5);
    assert_eq!(moving[0].position, base[0].position);
    let (before, third_person) = motion::camera(19, &moving);
    let first_person_actors = motion::actors(&base, 20);
    let (eye, first_person) = motion::camera(20, &first_person_actors);
    assert!(third_person.is_none());
    assert_eq!(first_person.unwrap().id, moving[2].id);
    assert_eq!(
        eye.position,
        first_person_actors[2].position + Vec3::Y * 1.6
    );
    assert_eq!(first_person_actors[2].pose[0], std::f32::consts::PI);
    assert!(eye.position.distance(before.position) > 2.0);
    assert!(motion::camera(24, &moving).1.is_none());
}

#[test]
fn depth_fixture_has_near_corners_stairs_and_explicit_emissive_reference() {
    let mut chunks = chunks();
    let camera = prepare(View::Depth, &mut chunks);
    assert!(camera.position.is_finite());
    assert_eq!(block(&chunks, [-3, 35, 0]), world::STONE);
    assert_eq!(block(&chunks, [0, 35, -7]), world::STONE);
    assert_eq!(block(&chunks, [6, 33, -5]), world::STONE);
    assert_eq!(block(&chunks, [6, 34, -5]), world::AIR);
    assert_eq!(block(&chunks, [6, 34, -6]), world::STONE);
    assert_eq!(block(&chunks, [1, 34, -7]), world::GLOWSTONE);
    // Returning to the standard scene rebuilds it; this fixture's emitter does
    // not leak into cave or canopy acceptance captures.
    prepare(View::Overview, &mut chunks);
    assert_eq!(block(&chunks, [1, 34, -7]), world::AIR);
}
