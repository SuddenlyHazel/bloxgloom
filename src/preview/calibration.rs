//! Matched lighting references: fixed geometry, camera, pose and exposure.
//! Every capture uses the production voxel/character, HDR and postprocess paths.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Scene {
    Outdoor,
    SkyShade,
    CaveEntry,
    SealedCave,
    LampCave,
    Night,
}

pub fn render_calibration_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("capture-settings.txt"),
        "Lighting calibration v1\nProduction WGPU voxel + articulated character + HDR/postprocess\n1280x800; exposure 1.0; fixed idle pose at 0.35s; fixed 45-degree camera\nLeft to right: light/black flat, dark/black defined, light/blond defined, dark/blond flat, light/pastel flat, dark/pastel defined\nVoxel lighting sampled at player feet + 1.45m, matching gameplay; bounce disabled\nNo UI, synthetic character fill, auto exposure, or per-shot exposure changes\n",
    )?;
    for (name, scene) in [
        ("01-outdoor-noon", Scene::Outdoor),
        ("02-open-sky-shade", Scene::SkyShade),
        ("03-cave-entrance", Scene::CaveEntry),
        ("04-sealed-cave", Scene::SealedCave),
        ("05-emissive-cave", Scene::LampCave),
        ("06-outdoor-night", Scene::Night),
    ] {
        let time = if matches!(scene, Scene::Night) {
            crate::daylight::CYCLE_MS * 3 / 4
        } else {
            crate::daylight::INITIAL_MS
        };
        pollster::block_on(render_previews_at(
            vec![PreviewOutput {
                path: directory.join(format!("{name}.png")),
                width: 1280,
                height: 800,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (0, 0),
            PreviewScene::Calibration(scene),
            None,
            time,
        ))?;
        println!("calibration capture: {name}");
    }
    Ok(())
}

pub(super) fn prepare(scene: Scene, chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) -> Camera {
    // A complete known flat field makes both geometry and direct-sky columns
    // independent of future procedural terrain edits.
    for (key, chunk) in chunks.iter_mut() {
        let mut blocks = vec![world::AIR; world::CHUNK_VOLUME];
        for y in 0..world::CHUNK_SIZE {
            let wy = key.y * world::CHUNK_SIZE as i32 + y as i32;
            if wy > 32 {
                continue;
            }
            for z in 0..world::CHUNK_SIZE {
                for x in 0..world::CHUNK_SIZE {
                    blocks[world::Chunk::index([x, y, z]).unwrap()] =
                        if wy == 32 { world::GRASS } else { world::STONE };
                }
            }
        }
        *chunk = Arc::new(world::Chunk::from_blocks(*key, 0, blocks));
    }
    for x in -1..=18 {
        for z in -23..=0 {
            set_preview_block(chunks, x, 32, z, world::STONE);
        }
    }
    // Repeated material steps expose upward, sun-facing and away-facing normals.
    for (x, block) in [
        (1, world::STONE),
        (4, world::SAND),
        (7, world::WOOD),
        (10, world::DIRT),
        (13, world::MOSS),
        (16, world::LEAVES),
    ] {
        for dx in 0..2 {
            for z in -21..=-19 {
                for y in 33..=34 {
                    set_preview_block(chunks, x + dx, y, z, block);
                }
            }
        }
        for z in -13..=-10 {
            for dx in 0..2 {
                set_preview_block(chunks, x + dx, 32, z, block);
            }
        }
    }
    // The open-sided roof removes direct sky while retaining a real sky portal.
    if matches!(
        scene,
        Scene::SkyShade | Scene::CaveEntry | Scene::SealedCave | Scene::LampCave
    ) {
        for x in -1..=18 {
            for z in -23..=-10 {
                set_preview_block(chunks, x, 38, z, world::STONE);
            }
        }
    }
    if matches!(
        scene,
        Scene::CaveEntry | Scene::SealedCave | Scene::LampCave
    ) {
        for y in 33..=38 {
            for z in -23..=if matches!(scene, Scene::CaveEntry) {
                -10
            } else {
                0
            } {
                for x in [-1, 18] {
                    set_preview_block(chunks, x, y, z, world::STONE);
                }
            }
            for x in -1..=18 {
                set_preview_block(chunks, x, y, -23, world::STONE);
            }
        }
        for x in -1..=18 {
            for z in -9..=if matches!(scene, Scene::CaveEntry) {
                -10
            } else {
                0
            } {
                set_preview_block(chunks, x, 38, z, world::STONE);
            }
        }
    }
    if matches!(scene, Scene::SealedCave | Scene::LampCave) {
        for x in -1..=18 {
            for y in 33..=38 {
                set_preview_block(chunks, x, y, 0, world::STONE);
            }
        }
    }
    if matches!(scene, Scene::LampCave) {
        // Actual emissive voxel sources, not hand-authored character light.
        for x in [2, 8, 14] {
            set_preview_block(chunks, x, 34, -18, world::GLOWSTONE);
        }
    }
    let position = Vec3::new(8.5, 35.5, -3.5);
    let target = Vec3::new(8.5, 34.0, -16.0);
    let direction = (target - position).normalize();
    Camera {
        position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 45f32.to_radians(),
    }
}

pub(super) fn avatars(chunks: &HashMap<ChunkKey, Arc<world::Chunk>>) -> Vec<render::VisualAvatar> {
    let mut fields = HashMap::new();
    [
        (4, 0, [18, 16, 21]),
        (3, 1, [18, 16, 21]),
        (4, 1, [236, 202, 123]),
        (3, 0, [236, 202, 123]),
        (4, 0, [190, 163, 219]),
        (3, 1, [190, 163, 219]),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (skin, body, hair_color))| {
        let appearance = crate::appearance::AppearanceState {
            palettes: [skin, 2, 1],
            character: Some(crate::appearance::CharacterRecipe {
                body,
                hair: if index < 2 {
                    1
                } else if index < 4 {
                    2
                } else {
                    4
                },
                hair_color,
                ..Default::default()
            }),
        };
        let appearance = crate::appearance::AppearanceState::decode(&appearance.encode()).unwrap();
        let position = Vec3::new(3.5 + index as f32 * 2.0, 33.0, -15.5);
        let p = (position + Vec3::Y * 1.45).floor().as_ivec3();
        let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
        let field = fields
            .entry(key)
            .or_insert_with(|| LightField::build_with_bounce(key, chunks, SEED, false));
        let sample = field.face(local, 1, 0);
        println!(
            "calibration actor {index}: sky={} glow={}",
            sample.sky, sample.glow
        );
        render::VisualAvatar {
            id: index as u64 + 1,
            model: render::AvatarModel::Player,
            animation: Default::default(),
            pose: [if index % 2 == 0 { -0.25 } else { 0.25 }, 0.0, 0.0, 0.0],
            motion: None,
            model_pose: None,
            character_pose: [0.0; 4],
            character_look: [0.0; 2],
            character_crouch: 0.0,
            character_tool: None,
            character_recipe: appearance.character,
            airborne: false,
            position,
            cosmetics: appearance.legacy(),
            light_levels: [sample.sky, sample.glow, 0, 0],
            bounce: [sample.bounce[0], sample.bounce[1], sample.bounce[2], 0],
            glow_bounce: [
                sample.glow_bounce[0],
                sample.glow_bounce[1],
                sample.glow_bounce[2],
                0,
            ],
            tint: [1.0; 3],
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_samples_distinguish_sky_portal_sealed_and_emissive_light() {
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
        for scene in [
            Scene::Outdoor,
            Scene::CaveEntry,
            Scene::SealedCave,
            Scene::LampCave,
        ] {
            prepare(scene, &mut chunks);
            let actors = avatars(&chunks);
            assert_eq!(actors.len(), 6);
            for actor in actors {
                let [sky, glow, _, _] = actor.light_levels;
                match scene {
                    Scene::Outdoor => assert_eq!([sky, glow], [15, 0]),
                    Scene::CaveEntry => assert!(sky > 0 && sky < 15 && glow == 0),
                    Scene::SealedCave => assert_eq!([sky, glow], [0, 0]),
                    Scene::LampCave => assert!(sky == 0 && glow > 0),
                    _ => unreachable!(),
                }
                assert!(actor.character_recipe.unwrap().valid());
            }
        }
    }
}
