use super::*;
#[test]
fn workshop_has_real_opening_enclosed_rear_and_bounded_emitters() {
    let mut catalog = crate::content::Catalog::builtins();
    sandbox::install_sandbox_materials(&mut catalog).unwrap();
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
                        vec![world::AIR; world::CHUNK_VOLUME],
                    )),
                );
            }
        }
    }
    let mut replay = chunks.clone();
    build(&mut chunks, &catalog);
    build(&mut replay, &catalog);
    for (key, chunk) in &chunks {
        for i in 0..world::CHUNK_VOLUME {
            assert_eq!(
                chunk.block_index(i),
                replay[key].block_index(i),
                "deterministic scene"
            );
        }
    }
    let block = |p: [i32; 3]| {
        let (key, local) = world::world_to_chunk(p[0], p[1], p[2]);
        chunks[&key].block(local).unwrap()
    };
    assert_eq!(block([1, 35, -8]), world::AIR);
    assert_eq!(block([1, 38, -8]), world::WOOD_X);
    assert_eq!(block([1, 35, -22]), world::SAND);
    assert_eq!(block([8, 35, -17]), world::AIR);
    assert_eq!(block([-3, 37, -21]), world::GLOWSTONE);
    assert_eq!(
        block([5, 37, -7]),
        catalog.state_by_key("sandbox:cyan").unwrap()
    );
    assert_eq!(
        block([3, 35, -20]),
        catalog.state_by_key("sandbox:steel").unwrap()
    );
    for &(x, z, h, _) in &vegetation::TREES {
        for y in 33..=33 + h {
            assert_eq!(block([x, y, z]), world::WOOD, "retained trunk anchor");
        }
    }
    for z in -4..=17 {
        let center = (z + 4) / 7;
        for x in center - 3..=center + 3 {
            assert_eq!(block([x, 33, z]), world::AIR, "walkable path verge");
        }
    }
    let emitters: usize = chunks
        .values()
        .map(|c| {
            (0..world::CHUNK_VOLUME)
                .filter(|i| catalog.emission(c.block_index(*i).unwrap()) > 0)
                .count()
        })
        .sum();
    assert_eq!(emitters, 3, "two warm practicals and one neon accent only");
    for view in [View::Approach, View::Doorway, View::Workbench] {
        let camera = camera(view);
        assert!(camera.position.is_finite());
        let p = camera.position.floor().as_ivec3();
        for dx in [-0.2, 0.2] {
            for dy in [-0.2, 0.2] {
                for dz in [-0.2, 0.2] {
                    assert_eq!(
                        block(
                            (camera.position + Vec3::new(dx, dy, dz))
                                .floor()
                                .as_ivec3()
                                .to_array()
                        ),
                        world::AIR,
                        "camera clearance volume"
                    );
                }
            }
        }
        assert_eq!(
            block([p.x, p.y, p.z]),
            world::AIR,
            "camera must not intersect scene"
        );
    }
}

#[test]
fn local_shadow_view_retains_wall_floor_and_real_task_lamp() {
    let mut catalog = crate::content::Catalog::builtins();
    sandbox::install_sandbox_materials(&mut catalog).unwrap();
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
                        vec![world::AIR; world::CHUNK_VOLUME],
                    )),
                );
            }
        }
    }
    build(&mut chunks, &catalog);
    local_shadow::prepare(&mut chunks);
    let block = |p: [i32; 3]| {
        let (key, local) = world::world_to_chunk(p[0], p[1], p[2]);
        chunks[&key].block(local).unwrap()
    };
    assert_eq!(block([1, 34, -16]), world::GLOWSTONE);
    assert_eq!(block([-3, 37, -21]), world::AIR);
    assert_eq!(block([-6, 34, -17]), world::SAND);
    assert_eq!(block([-2, 32, -16]), world::WOOD_X);
    for x in -5..=0 {
        assert_eq!(block([x, 34, -16]), world::AIR);
    }
    let off = camera(View::LocalShadow { enabled: false });
    let on = camera(View::LocalShadow { enabled: true });
    assert_eq!(off.position, on.position);
    assert_eq!(off.yaw, on.yaw);
    assert_eq!(off.pitch, on.pitch);
    assert_eq!(off.fov_y_radians, on.fov_y_radians);
    assert_eq!(
        block(off.position.floor().as_ivec3().to_array()),
        world::AIR
    );
}
