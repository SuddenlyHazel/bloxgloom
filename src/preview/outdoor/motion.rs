//! Bounded temporal stress capture: real character skinning, moving cutout geometry,
//! camera translation, a first-person cut, and resize. This is not a benchmark.
use super::*;

pub(crate) const FRAMES: usize = 28;

pub fn render(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("sequence.txt"),
        format!(
            "Outdoor temporal motion v3\nAA requested: {}\nObject motion: previous submitted character/GLB skin palettes; reactive production foliage wind\nContact occlusion request: {} (default 1)\nFrames 00-07 stationary warmup; 08-19 lateral camera pan, walking/translating actors, production GPU foliage wind; 20-23 first-person cut and head clipping; 23 resize from 640x400 to 800x500; 24-27 return-to-canopy cut\nOne submitted sample per frame, persistent history, nominal 30 Hz animation\nFoliage uses production world-space GPU wind with anchored roots, paired-plant continuity and explicit temporal history rejection. Color, sun/local depth casters and ray intersections share the deformation and clock. No per-frame CPU foliage mesh uploads. Character contact and sun shadows follow each frame.\nUse the same adapter, shadow settings, and executable for off/on comparison. Software GPU captures establish visual behavior, not performance.\n",
            if render::post::temporal_requested() {
                "1"
            } else {
                "0"
            },
            std::env::var("BLOXGLOOM_CONTACT_OCCLUSION").unwrap_or_else(|_| "1".into())
        ),
    )?;
    let outputs = (0..FRAMES)
        .map(|frame| {
            let (width, height) = size(frame);
            PreviewOutput {
                path: directory.join(format!("{frame:02}.png")),
                width,
                height,
                scale: 1.0,
                screen: UiScreen::Playing,
                orientation: None,
            }
        })
        .collect();
    pollster::block_on(render_previews_at(
        outputs,
        (0, 0),
        PreviewScene::Outdoor(View::Motion),
        None,
        crate::daylight::INITIAL_MS,
    ))
}

pub(crate) fn size(frame: usize) -> (u32, u32) {
    if frame >= 23 { (800, 500) } else { (640, 400) }
}

pub(crate) fn seconds(frame: usize) -> f32 {
    frame.saturating_sub(7) as f32 / 30.0
}

pub(crate) fn actors(base: &[render::VisualAvatar], frame: usize) -> Vec<render::VisualAvatar> {
    let mut actors = base.to_vec();
    let distance = frame.saturating_sub(7).min(12) as f32 * 0.18;
    actors[2].position.x += distance;
    actors[3].position.x -= distance * 0.6;
    for (index, actor) in actors.iter_mut().enumerate().skip(6) {
        actor.position.x += distance * if index % 2 == 0 { 0.65 } else { -0.45 };
        if let Some(visual) = &mut actor.model_pose {
            visual.sample_tick = frame.saturating_sub(7) as u64 * 2;
        }
    }
    if (20..24).contains(&frame) {
        // First-person framing expects the owner's body yaw to match the eye.
        actors[2].pose[0] = std::f32::consts::PI - (frame - 20) as f32 * 0.04;
    }
    actors
}

pub(crate) fn camera(
    frame: usize,
    actors: &[render::VisualAvatar],
) -> (Camera, Option<render::FirstPersonView>) {
    if (20..24).contains(&frame) {
        let camera = Camera {
            position: actors[2].position + Vec3::Y * 1.6,
            yaw: -std::f32::consts::FRAC_PI_2 + (frame - 20) as f32 * 0.04,
            pitch: -0.35,
            fov_y_radians: 70.0f32.to_radians(),
        };
        return (
            camera,
            Some(render::FirstPersonView {
                id: actors[2].id,
                eye_height: 1.6,
                pitch: camera.pitch,
            }),
        );
    }
    let mut camera = super::camera(View::Canopy);
    camera.position.x += frame.saturating_sub(7).min(12) as f32 * 0.08;
    (camera, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequence_contains_warmup_continuous_motion_cut_and_resize() {
        assert_eq!(seconds(0), 0.0);
        assert_eq!(seconds(7), 0.0);
        assert!(seconds(8) > 0.0);
        assert_eq!(size(22), (640, 400));
        assert_eq!(size(23), (800, 500));
        assert_eq!(FRAMES, 28);
    }
}
