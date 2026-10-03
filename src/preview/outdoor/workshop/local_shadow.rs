//! Matched local-light shadow acceptance: actual workshop surfaces and animated casters.
use super::*;

pub(in crate::preview) const FRAMES: usize = 8;

pub fn render(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("capture-settings.txt"),
        "Workshop local shadow acceptance v1\nProduction world, articulated character and registered GLB render paths\n960x600, fixed camera, dusk phase .48, exposure 1.0, bloom .12, bounce disabled\nEight matched off/on frames at 8 Hz. Actor translates in front of a real glowstone task lamp, projecting onto the workshop wall and floor. GLB fixture also animates and casts.\nOff/on disables only the local shadow maps; contact and directional shadow settings remain identical. Existing assets only, no painted shadows, extra fill or exposure changes.\nSoftware GPU captures are visual evidence only, never performance evidence.\n",
    )?;
    for enabled in [false, true] {
        let name = if enabled { "on" } else { "off" };
        let outputs = (0..FRAMES)
            .map(|frame| PreviewOutput {
                path: directory.join(format!("{name}-{frame:02}.png")),
                width: 960,
                height: 600,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            })
            .collect();
        pollster::block_on(render_previews_at(
            outputs,
            (0, 0),
            PreviewScene::Outdoor(super::super::View::Workshop(View::LocalShadow { enabled })),
            None,
            crate::daylight::CYCLE_MS * 48 / 100,
        ))?;
    }
    Ok(())
}

pub(super) fn prepare(chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) {
    let mut s = Builder { chunks };
    // Move one existing practical to a timber task-lamp stand. No hidden fill.
    s.put([-3, 37, -21], world::AIR);
    s.put([1, 33, -16], world::WOOD);
    s.put([1, 34, -16], world::GLOWSTONE);
    // Clear the kiln from this task-lamp receiver area. The workshop keeps
    // its existing floor, plaster wall and material library.
    s.fill([-5, 33, -18], [-4, 35, -14], world::AIR);
}

pub(in crate::preview) fn actors(
    chunks: &HashMap<ChunkKey, Arc<world::Chunk>>,
    frame: usize,
) -> Vec<render::VisualAvatar> {
    let mut actors = calibration::avatars(chunks);
    actors.truncate(1);
    let distance = frame.min(FRAMES - 1) as f32 * 0.22;
    actors[0].position = Vec3::new(-2.0, 33.0, -16.5 + distance);
    actors[0].pose[0] = 0.35;
    if let Some(kind) = crate::content::catalog().entity_type_id_by_key("preview:outdoor-creature")
    {
        actors.push(render::VisualAvatar {
            id: 10_000,
            model: render::AvatarModel::Registered(kind),
            model_pose: Some(bloxgloom_host_api::entity::VisualState {
                sample_tick: frame as u64 * 8,
                transition_s: 0.0,
                ..Default::default()
            }),
            position: Vec3::new(-1.8, 33.0, -18.1),
            character_recipe: None,
            pose: [0.0; 4],
            ..actors[0]
        });
    }
    for actor in &mut actors {
        let p = (actor.position + Vec3::Y * 1.0).floor().as_ivec3();
        let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
        let sample = LightField::build_with_bounce(key, chunks, SEED, false).face(local, 1, 0);
        actor.light_levels = [sample.sky, sample.glow, 0, 0];
        actor.glow_color = sample.glow_color;
        actor.glow_direction = sample.glow_direction;
        actor.bounce = [0; 4];
        actor.glow_bounce = [0; 4];
    }
    actors
}
