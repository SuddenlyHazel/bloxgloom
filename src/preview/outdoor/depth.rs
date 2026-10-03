//! A compact material-neutral scene for near-field depth and indirect AO.
use super::*;

pub fn render(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("capture-settings.txt"),
        "Depth acceptance: 1280x800, noon, exposure 1, bloom 0.12.\nStone floor, perpendicular walls, stairs, inset emissive blocks and registered GLB creature.\nCompare BLOXGLOOM_AO=0/0.75 (radius 1.5) with all other settings and binary fixed.\nDirect lighting and emissive surface radiance must remain separate from indirect occlusion.\n",
    )?;
    pollster::block_on(render_previews_at(
        vec![PreviewOutput {
            path: directory.join("01-depth-courtyard.png"),
            width: 1280,
            height: 800,
            scale: 1.0,
            screen: UiScreen::Playing,
            orientation: None,
        }],
        (0, 0),
        PreviewScene::Outdoor(View::Depth),
        None,
        crate::daylight::INITIAL_MS,
    ))
}

pub(super) fn prepare(chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) {
    for x in -3..=8 {
        for z in -7..=2 {
            set_preview_block(chunks, x, 32, z, world::STONE);
        }
    }
    for y in 33..=38 {
        for x in -3..=8 {
            set_preview_block(chunks, x, y, -7, world::STONE);
        }
        for z in -6..=2 {
            set_preview_block(chunks, -3, y, z, world::STONE);
        }
    }
    // Uneven-height, close-set steps expose contact, concavity and distance fade.
    for (x0, x1, z0, z1, height) in [(5, 7, -5, -4, 1), (5, 7, -6, -6, 2), (-2, -1, -5, -4, 2)] {
        for x in x0..=x1 {
            for z in z0..=z1 {
                for y in 33..33 + height {
                    set_preview_block(chunks, x, y, z, world::STONE);
                }
            }
        }
    }
    for x in [1, 2] {
        set_preview_block(chunks, x, 34, -7, world::GLOWSTONE);
    }
}
