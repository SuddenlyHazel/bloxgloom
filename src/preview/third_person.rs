//! Offscreen verification of the same camera sweep and character renderer as gameplay.
use super::*;
use crate::render::camera::Perspective;

#[derive(Clone, Copy)]
pub(super) struct Shot {
    pub(super) perspective: Perspective,
    pub(super) wall: bool,
    pub(super) animation: GameplayPose,
    pub(super) pitch: Option<f32>,
}

#[derive(Clone, Copy)]
pub(super) enum GameplayPose {
    Idle,
    Walk,
    Crouch,
    CrouchWalk,
    Tool(bool),
    CrouchWalkTool,
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
            PreviewScene::ThirdPerson(Shot {
                perspective,
                wall,
                animation: GameplayPose::Idle,
                pitch: None,
            }),
        ))?;
    }
    Ok(())
}

pub fn render_gameplay_animation_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, animation) in [
        ("walk.png", GameplayPose::Walk),
        ("crouch.png", GameplayPose::Crouch),
        ("crouch-walk.png", GameplayPose::CrouchWalk),
        ("tool-left.png", GameplayPose::Tool(false)),
        ("tool-right.png", GameplayPose::Tool(true)),
        ("crouch-walk-tool.png", GameplayPose::CrouchWalkTool),
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
            PreviewScene::ThirdPerson(Shot {
                perspective: Perspective::Front,
                wall: false,
                animation,
                pitch: None,
            }),
        ))?;
    }
    Ok(())
}

pub fn render_first_person_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (name, animation, pitch) in [
        ("forward.png", GameplayPose::Idle, 0.0),
        ("look-down.png", GameplayPose::Idle, -1.48),
        ("walk-down.png", GameplayPose::Walk, -1.48),
        ("crouch-down.png", GameplayPose::Crouch, -1.48),
        ("tool.png", GameplayPose::Tool(true), 0.0),
        ("tool-down.png", GameplayPose::Tool(true), -0.7),
        ("crouch-tool.png", GameplayPose::CrouchWalkTool, 0.0),
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
            PreviewScene::ThirdPerson(Shot {
                perspective: Perspective::FirstPerson,
                wall: false,
                animation,
                pitch: Some(pitch),
            }),
        ))?;
    }
    Ok(())
}

impl Shot {
    pub(super) fn eye_height(self) -> f32 {
        let crouching = matches!(
            self.animation,
            GameplayPose::Crouch | GameplayPose::CrouchWalk | GameplayPose::CrouchWalkTool
        );
        bloxgloom_host_api::player::BUILTIN_RULES
            .for_stance(crouching)
            .eye_height()
    }
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
            height as f32 + 1.0 + shot.eye_height(),
            target.1 as f32 + 0.5,
        ),
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: shot.pitch.unwrap_or(-0.12),
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
    } else if shot.perspective != Perspective::FirstPerson {
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

pub(super) fn avatar(shot: Shot, target: (i32, i32), height: i32) -> render::VisualAvatar {
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
        motion: None,
        character_pose: [
            0.2,
            0.35,
            if matches!(
                shot.animation,
                GameplayPose::Walk | GameplayPose::CrouchWalk | GameplayPose::CrouchWalkTool
            ) {
                1.0
            } else {
                0.0
            },
        ],
        character_crouch: if matches!(
            shot.animation,
            GameplayPose::Crouch | GameplayPose::CrouchWalk | GameplayPose::CrouchWalkTool
        ) {
            1.0
        } else {
            0.0
        },
        character_tool: match shot.animation {
            GameplayPose::Tool(right) => Some((right, 0.4)),
            GameplayPose::CrouchWalkTool => Some((true, 0.4)),
            _ => None,
        },
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
