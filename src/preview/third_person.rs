//! Offscreen verification of the same camera sweep and character renderer as gameplay.
use super::*;
use crate::render::camera::Perspective;

#[derive(Clone, Copy)]
pub(super) struct Shot {
    pub(super) perspective: Perspective,
    pub(super) wall: bool,
}

pub fn render_third_person_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, perspective, wall) in [
        ("behind.png", Perspective::Behind, false),
        ("front.png", Perspective::Front, false),
        ("wall-retracted.png", Perspective::Behind, true),
    ] {
        pollster::block_on(render_previews(
            vec![PreviewOutput {
                path: directory.join(name),
                width: 1280,
                height: 720,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }],
            (0, 0),
            PreviewScene::ThirdPerson(Shot { perspective, wall }),
        ))?;
    }
    Ok(())
}

pub(super) fn prepare(
    shot: Shot,
    chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>,
    target: (i32, i32),
    height: i32,
) -> Camera {
    for x in target.0 - 6..=target.0 + 6 {
        for z in target.1 - 6..=target.1 + 6 {
            for y in height - 2..=height + 7 {
                let block = if y > height {
                    world::AIR
                } else if y == height {
                    world::GRASS
                } else {
                    world::DIRT
                };
                set_preview_block(chunks, x, y, z, block);
            }
        }
    }
    if shot.wall {
        for x in target.0 - 3..=target.0 + 3 {
            for y in height + 1..=height + 6 {
                set_preview_block(chunks, x, y, target.1 + 2, world::STONE);
            }
        }
    }
    let eye = Camera {
        position: Vec3::new(
            target.0 as f32 + 0.5,
            height as f32 + 2.62,
            target.1 as f32 + 0.5,
        ),
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: -0.12,
        fov_y_radians: 70f32.to_radians(),
    };
    let camera = shot.perspective.view(eye, |position| {
        let (key, local) = world::world_to_chunk(position[0], position[1], position[2]);
        chunks
            .get(&key)
            .and_then(|chunk| chunk.block(local))
            .is_none_or(world::is_solid)
    });
    let distance = camera.position.distance(eye.position);
    if shot.wall {
        assert!(
            distance > 0.5 && distance < 1.5,
            "camera did not retract before wall: {distance}"
        );
    } else {
        assert!(
            (distance - 4.0).abs() < 0.05,
            "clear camera did not extend: {distance}"
        );
    }
    println!(
        "third-person {:?}, wall={}: camera distance {distance:.3}",
        shot.perspective, shot.wall
    );
    camera
}

pub(super) fn avatar(target: (i32, i32), height: i32) -> render::VisualAvatar {
    // Go through the canonical public appearance codec instead of supplying an
    // unvalidated recipe directly to the renderer.
    let appearance = crate::appearance::AppearanceState {
        palettes: [1, 2, 3],
        character: Some(crate::appearance::CharacterRecipe {
            hair: 2,
            eyes: 5,
            mouth: 3,
            iris: Some([36, 220, 95]),
        }),
    };
    let appearance = crate::appearance::AppearanceState::decode(&appearance.encode()).unwrap();
    render::VisualAvatar {
        id: 1,
        model: render::AvatarModel::Player,
        animation: Default::default(),
        pose: [std::f32::consts::PI, 0.0, 0.0, 0.0],
        character_pose: [0.0, 0.35, 0.0],
        character_recipe: appearance.character,
        airborne: false,
        position: Vec3::new(
            target.0 as f32 + 0.5,
            height as f32 + 1.0,
            target.1 as f32 + 0.5,
        ),
        cosmetics: appearance.legacy(),
        light_levels: [15, 0, 0, 0],
        bounce: [0; 4],
        glow_bounce: [0; 4],
        tint: [1.0; 3],
    }
}
