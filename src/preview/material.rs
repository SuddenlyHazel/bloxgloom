//! Close, unobstructed material comparisons through production meshing and shading.
use super::*;

pub fn render_material_previews(key: &str, directory: &Path) -> Result<(), Box<dyn Error>> {
    let state = crate::content::catalog()
        .state_by_key(key)
        .ok_or("unknown legal block state")?;
    fs::create_dir_all(directory)?;
    for (light, time) in [
        ("morning", crate::daylight::CYCLE_MS / 8),
        ("noon", crate::daylight::INITIAL_MS),
    ] {
        for (name, mode) in [
            ("albedo", render::MaterialPreviewMode::Albedo),
            ("normals", render::MaterialPreviewMode::Normals),
            ("full", render::MaterialPreviewMode::Full),
        ] {
            let path = directory.join(format!("{light}-{name}.png"));
            pollster::block_on(render_previews_at(
                vec![PreviewOutput {
                    path,
                    width: 1000,
                    height: 800,
                    scale: 1.0,
                    screen: UiScreen::Playing,
                    orientation: None,
                }],
                (0, 0),
                PreviewScene::Material(state, mode),
                None,
                time,
            ))?;
        }
    }
    Ok(())
}

pub(super) fn prepare(
    state: crate::content::BlockStateId,
    chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>,
) -> Camera {
    for (key, chunk) in chunks.iter_mut() {
        let mut blocks = vec![world::AIR; world::CHUNK_VOLUME];
        let local_y = 32 - key.y * world::CHUNK_SIZE as i32;
        if (0..world::CHUNK_SIZE as i32).contains(&local_y) {
            for z in 0..world::CHUNK_SIZE {
                for x in 0..world::CHUNK_SIZE {
                    blocks[world::Chunk::index([x, local_y as usize, z]).unwrap()] = world::STONE;
                }
            }
        }
        *chunk = Arc::new(world::Chunk::from_blocks(*key, 0, blocks));
    }
    set_preview_block(chunks, 0, 33, 0, state);
    let target = Vec3::new(0.5, 33.5, 0.5);
    let position = target + Vec3::new(2.0, 1.5, 2.5);
    let direction = (target - position).normalize();
    Camera {
        position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: 35f32.to_radians(),
    }
}
