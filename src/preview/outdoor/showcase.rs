//! Fixed JG RTX acceptance scene: identical geometry in every lighting comparison.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(in crate::preview) enum View {
    Overview,
    Bark,
    Stream,
    Motion,
}

pub fn render_motion(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    let outputs = (0..30)
        .map(|frame| PreviewOutput {
            path: directory.join(format!("frame-{frame:02}.png")),
            width: 1000,
            height: 625,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        })
        .collect();
    pollster::block_on(render_previews_at(
        outputs,
        (0, 0),
        PreviewScene::Outdoor(super::View::Showcase(View::Motion)),
        None,
        crate::daylight::INITIAL_MS,
    ))
}

pub fn render(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    for (light, time) in [
        ("noon", crate::daylight::INITIAL_MS),
        ("golden", crate::daylight::CYCLE_MS / 16),
    ] {
        for (name, view) in [
            ("overview", View::Overview),
            ("bark", View::Bark),
            ("stream", View::Stream),
        ] {
            pollster::block_on(render_previews_at(
                vec![PreviewOutput {
                    path: directory.join(format!("{light}-{name}.png")),
                    width: 1280,
                    height: 800,
                    scale: 1.0,
                    screen: UiScreen::Playing,
                    orientation: None,
                }],
                (0, 0),
                PreviewScene::Outdoor(super::View::Showcase(view)),
                None,
                time,
            ))?;
        }
    }
    fs::write(
        directory.join("capture-settings.txt"),
        format!(
            "JG RTX showcase v1: fixed geometry/cameras, 1280x800, production materials and shadows. Noon and early morning. AA requested: {}. Defaults use exposure 1; explicit preview config: {}. No saved world modified.\n",
            u8::from(render::post::temporal_requested()),
            std::env::var("BLOXGLOOM_PREVIEW_CONFIG").unwrap_or_else(|_| "none".into())
        ),
    )?;
    Ok(())
}

fn state(key: &str) -> crate::content::BlockStateId {
    crate::content::catalog()
        .state_by_key(&format!("bloxgloom:{key}"))
        .unwrap_or_else(|| panic!("showcase material missing: {key}"))
}

pub(super) fn prepare(view: View, chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) -> Camera {
    let cherry = state("cherry_log[axis=y]");
    let leaves = state("cherry_leaves");
    let planks = state("cherry_planks");
    let bricks = state("stone_bricks");
    let moss = state("mossy_stone_bricks");
    let copper = state("copper_ore");
    // A shallow stream with visible gravel bed and grass banks.
    for x in -24..=24 {
        for z in -30..=12 {
            if (3..=7).contains(&x) {
                set_preview_block(chunks, x, 30, z, world::GRAVEL);
                set_preview_block(chunks, x, 31, z, world::WATER);
                set_preview_block(chunks, x, 32, z, world::WATER);
            }
        }
    }
    for (x, z) in [(-3, -2), (-12, -13), (-4, -18), (15, -20)] {
        for y in 33..=39 {
            set_preview_block(chunks, x, y, z, cherry);
        }
        for (y, radius) in [(38, 3i32), (39, 4), (40, 4), (41, 3), (42, 1)] {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() + dz.abs() <= radius + 1 {
                        set_preview_block(chunks, x + dx, y, z + dz, leaves);
                    }
                }
            }
        }
    }
    // Small masonry shelter with a shaded recess and a wooden roof.
    for x in -18..=-9 {
        for z in -26..=-20 {
            set_preview_block(chunks, x, 33, z, bricks);
            for y in 34..=38 {
                if x == -18 || x == -9 || z == -26 || (z == -20 && !(-15..=-12).contains(&x)) {
                    set_preview_block(chunks, x, y, z, if y == 34 { moss } else { bricks });
                }
            }
            set_preview_block(chunks, x, 39, z, planks);
        }
    }
    for x in 1..=9 {
        for z in -10..=-8 {
            set_preview_block(chunks, x, 33, z, planks);
        }
    }
    for (x, block) in [(-7, copper), (-6, bricks), (-5, planks)] {
        set_preview_block(chunks, x, 33, -7, block);
        set_preview_block(chunks, x, 34, -7, block);
    }
    for (x, z, key) in [
        (-1, 1, "poppy"),
        (-6, 0, "dandelion"),
        (0, -7, "blue_orchid"),
        (-8, -10, "large_fern[half=lower]"),
    ] {
        set_preview_block(chunks, x, 33, z, state(key));
    }
    camera(view)
}

pub(super) fn camera(view: View) -> Camera {
    let (position, target, fov) = match view {
        View::Overview | View::Motion => (
            Vec3::new(17.0, 39.0, 10.0),
            Vec3::new(-5.0, 35.0, -12.0),
            60f32,
        ),
        View::Bark => (
            Vec3::new(0.8, 35.1, 2.4),
            Vec3::new(-2.5, 35.1, -1.5),
            45f32,
        ),
        View::Stream => (
            Vec3::new(8.5, 34.3, 1.5),
            Vec3::new(4.5, 32.5, -13.0),
            58f32,
        ),
    };
    let direction = (target - position).normalize();
    Camera {
        position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: fov.to_radians(),
    }
}
