//! Outdoor acceptance fixture: one authored scene, fixed cameras and exposure.
//! No saved world, procedural terrain, per-shot lighting or synthetic character fill.
use super::*;
mod creatures;
mod depth;
pub(super) mod motion;
pub(super) mod showcase;
pub(super) mod workshop;
pub use creatures::install as install_outdoor_creatures;
pub use depth::render as render_outdoor_depth;
pub use motion::render as render_outdoor_motion;

#[derive(Clone, Copy, Debug)]
pub(super) enum View {
    Showcase(showcase::View),
    Workshop(workshop::View),
    Overview,
    Canopy,
    Entrance,
    Interior,
    Motion,
    Depth,
}

pub fn render_outdoor_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("capture-settings.txt"),
        format!(
            "Outdoor acceptance v1\nProduction voxel, articulated character, sun-shadow and HDR/postprocess paths\n1280x800; exposure 1.0; bloom 0.12; bounce disabled\nNoon world time {}; idle pose 0.35 seconds; no UI or auto exposure\nTemporal AA requested: {}; eight stationary samples on supported backends; GL fallback is reported on stderr\nContact occlusion requested: {} (finite strength clamped 0..1; default 1)\nShadow quality: {:?}; set BLOXGLOOM_SUN_SHADOWS=high for matched acceptance captures\nOne identical scene in all four views, built from builtin grass, stone, wood and alpha-cutout leaves\nOverview: open grass, overlapping canopy and cave mouth\nCanopy: light/dark character recipes under layered leaves\nEntrance: light/dark character recipes and unlit deep cave\nInterior: no emissives or exposure compensation; deep stone must remain dark\nAdapter identity and actor sky/glow samples are printed to stdout\nSoftware rendering is visual evidence, not hardware performance evidence\n",
            crate::daylight::INITIAL_MS,
            if render::post::temporal_requested() {
                "on"
            } else {
                "off"
            },
            std::env::var("BLOXGLOOM_CONTACT_OCCLUSION").unwrap_or_else(|_| "1".into()),
            sun_shadow::quality()?,
        ),
    )?;
    for (name, view) in [
        ("01-overview", View::Overview),
        ("02-canopy-character", View::Canopy),
        ("03-cave-entrance", View::Entrance),
        ("04-dark-interior", View::Interior),
    ] {
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
            PreviewScene::Outdoor(view),
            None,
            crate::daylight::INITIAL_MS,
        ))?;
        println!("outdoor capture: {name}");
    }
    Ok(())
}

pub(super) fn prepare(view: View, chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) -> Camera {
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
    if let View::Workshop(view) = view {
        return workshop::prepare(view, chunks);
    }
    if let View::Showcase(view) = view {
        return showcase::prepare(view, chunks);
    }
    // Overlapping, volumetric crowns rather than one alpha plane. The near edge
    // exposes leaf silhouettes; the center stacks five layers above the actors.
    for (x, z) in [(-12, -15), (-5, -16), (-13, -7), (-4, -8)] {
        for y in 33..=39 {
            set_preview_block(chunks, x, y, z, world::WOOD);
        }
        for (y, radius) in [(38, 4i32), (39, 5), (40, 5), (41, 4), (42, 2)] {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() + dz.abs() <= radius + 2 {
                        set_preview_block(chunks, x + dx, y, z + dz, world::LEAVES);
                    }
                }
            }
        }
    }
    // Solid hill with one long portal. No light source is hidden inside.
    for x in 11..=27 {
        for z in -30..=-8 {
            for y in 33..=42 {
                let tunnel = (14..=23).contains(&x) && (33..=37).contains(&y) && z > -29;
                if !tunnel {
                    set_preview_block(chunks, x, y, z, world::STONE);
                }
            }
            set_preview_block(chunks, x, 43, z, world::GRASS);
        }
    }
    // A pair of interior stone steps gives the dark reference visible geometry
    // without introducing artificial illumination.
    for x in [16, 21] {
        set_preview_block(chunks, x, 33, -26, world::STONE);
        set_preview_block(chunks, x, 34, -27, world::STONE);
    }
    if matches!(view, View::Depth) {
        depth::prepare(chunks);
    }
    camera(view)
}

fn camera(view: View) -> Camera {
    let (position, target, fov) = match view {
        View::Showcase(view) => return showcase::camera(view),
        View::Workshop(view) => return workshop::camera(view),
        View::Depth => (Vec3::new(10.0, 37.5, 8.5), Vec3::new(1.0, 34.0, -3.5), 55.0),
        View::Overview => (
            Vec3::new(30.0, 48.0, 27.0),
            Vec3::new(1.0, 35.0, -10.0),
            55.0f32,
        ),
        View::Canopy | View::Motion => (
            Vec3::new(-8.0, 35.5, -0.5),
            Vec3::new(-8.0, 34.4, -11.5),
            52.0,
        ),
        View::Entrance => (
            Vec3::new(18.5, 36.0, 4.0),
            Vec3::new(18.5, 34.5, -15.0),
            50.0,
        ),
        View::Interior => (
            Vec3::new(18.5, 35.2, -15.0),
            Vec3::new(18.5, 34.5, -27.0),
            55.0,
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

pub(super) fn avatars(chunks: &HashMap<ChunkKey, Arc<world::Chunk>>) -> Vec<render::VisualAvatar> {
    let mut actors = calibration::avatars(chunks);
    // Reuse the established six calibration recipes, two per lighting condition.
    for (actor, position) in actors.iter_mut().zip([
        [1.5, 33.0, 0.0],
        [4.0, 33.0, 0.0],
        [-9.5, 33.0, -11.5],
        [-7.0, 33.0, -11.5],
        [17.0, 33.0, -12.5],
        [20.0, 33.0, -12.5],
    ]) {
        actor.position = Vec3::from_array(position);
        let p = (actor.position + Vec3::Y * 1.45).floor().as_ivec3();
        let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
        let sample = LightField::build_with_bounce(key, chunks, SEED, false).face(local, 1, 0);
        actor.light_levels = [sample.sky, sample.glow, 0, 0];
        actor.glow_color = sample.glow_color;
        actor.glow_direction = sample.glow_direction;
        actor.bounce = [0; 4];
        actor.glow_bounce = [0; 4];
        println!(
            "outdoor actor {} at {:?}: sky={} glow={}",
            actor.id, position, sample.sky, sample.glow
        );
    }
    creatures::append(&mut actors, chunks);
    actors
}

#[cfg(test)]
mod tests;
