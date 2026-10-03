//! A bounded representative scene, sharing production lighting and existing assets.
use super::*;

pub(in crate::preview) mod local_shadow;
mod vegetation;

#[derive(Clone, Copy, Debug)]
pub(in crate::preview) enum View {
    Approach,
    Doorway,
    Workbench,
    LocalShadow { enabled: bool },
}

pub fn render_workshop_previews(directory: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join("capture-settings.txt"),
        format!(
            "Woodland workshop v1\nProduction game renderer; existing builtin and sandbox materials only\n1280x800; exposure 1.0; bloom 0.12; bounce disabled; idle 0.35s\nMatched scene/camera pairs; daylight phase .25 and dusk .48\nShadow quality {:?}; TAA requested {}; AO requested {}; contact occlusion {}\nNo per-shot lighting or exposure overrides, external assets, world saves, or performance measurements\n",
            sun_shadow::quality()?,
            std::env::var("BLOXGLOOM_TAA").unwrap_or_else(|_| "0".into()),
            std::env::var("BLOXGLOOM_AO").unwrap_or_else(|_| "default".into()),
            std::env::var("BLOXGLOOM_CONTACT_OCCLUSION").unwrap_or_else(|_| "1".into())
        ),
    )?;
    for (name, view) in [
        ("01-approach", View::Approach),
        ("02-doorway", View::Doorway),
        ("03-workbench", View::Workbench),
    ] {
        for (time, ms) in [
            ("daylight", crate::daylight::INITIAL_MS),
            ("dusk", crate::daylight::CYCLE_MS * 48 / 100),
        ] {
            pollster::block_on(render_previews_at(
                vec![PreviewOutput {
                    path: directory.join(format!("{name}-{time}.png")),
                    width: 1280,
                    height: 800,
                    scale: 1.0,
                    screen: UiScreen::Playing,
                    orientation: None,
                }],
                (0, 0),
                PreviewScene::Outdoor(super::View::Workshop(view)),
                None,
                ms,
            ))?;
            println!("workshop capture: {name}-{time}");
        }
    }
    Ok(())
}

pub(super) fn camera(view: View) -> Camera {
    let (position, target, fov) = match view {
        View::LocalShadow { .. } => (
            Vec3::new(4.5, 36.2, -10.5),
            Vec3::new(-2.3, 34.4, -16.0),
            58.0f32,
        ),
        View::Approach => (
            Vec3::new(23.0, 40.5, 17.0),
            Vec3::new(2.0, 36.7, -11.5),
            52.0f32,
        ),
        View::Doorway => (
            Vec3::new(-6.0, 35.6, 3.0),
            Vec3::new(1.0, 35.3, -13.0),
            57.0,
        ),
        View::Workbench => (
            Vec3::new(3.5, 35.5, -9.0),
            Vec3::new(0.0, 35.0, -18.5),
            66.0,
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

pub(super) fn prepare(view: View, chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) -> Camera {
    build(chunks, crate::content::catalog());
    if matches!(view, View::LocalShadow { .. }) {
        local_shadow::prepare(chunks);
    }
    camera(view)
}

fn build(chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>, catalog: &crate::content::Catalog) {
    let steel = catalog
        .state_by_key("sandbox:steel")
        .expect("registered workshop steel");
    let cyan = catalog
        .state_by_key("sandbox:cyan")
        .expect("registered workshop cyan");
    let mut s = Builder { chunks };
    // Broken path into a modest timber-frame building, with planted verges.
    for z in -22i32..=17 {
        let center = if z > -4 { (z + 4) / 7 } else { 0 };
        for x in center - 2..=center + 2 {
            s.put(
                [x, 32, z],
                if (x * 7 + z * 13).rem_euclid(9) < 2 {
                    world::STONE
                } else {
                    world::GRAVEL
                },
            );
        }
    }
    s.fill([-6, 32, -22], [8, 32, -6], world::STONE);
    s.fill([-5, 32, -21], [7, 32, -7], world::WOOD_X);
    // Plaster infill and explicit timber framing: a true enclosed rear room.
    s.fill([-6, 33, -22], [8, 38, -22], world::SAND);
    s.fill([-6, 33, -21], [-6, 38, -8], world::SAND);
    s.fill([8, 33, -21], [8, 38, -8], world::SAND);
    s.fill([-6, 33, -8], [-3, 38, -8], world::SAND);
    s.fill([4, 33, -8], [8, 38, -8], world::SAND);
    for x in [-6, -3, 4, 8] {
        s.fill([x, 33, -8], [x, 39, -8], world::WOOD);
        s.fill([x, 33, -22], [x, 39, -22], world::WOOD);
    }
    for z in [-22, -15, -8] {
        s.fill([-6, 33, z], [-6, 39, z], world::WOOD);
        s.fill([8, 33, z], [8, 39, z], world::WOOD);
    }
    s.fill([-6, 38, -8], [8, 38, -8], world::WOOD_X);
    s.fill([-6, 38, -22], [8, 38, -22], world::WOOD_X);
    // Roof slopes to either side, deep eaves shelter the open doorway.
    for x in -8i32..=10 {
        let y = 39 + (8 - (x - 1).abs()).max(0) / 2;
        s.fill([x, y, -24], [x, y, -5], world::WOOD_Z);
        if y > 39 {
            s.fill([x, 39, -22], [x, y - 1, -22], world::SAND);
        }
    }
    // Glazed-free side window, framing and sill. Looking through tests indirect depth.
    s.fill([8, 35, -18], [8, 36, -16], world::AIR);
    s.fill([8, 34, -19], [8, 34, -15], world::WOOD_Z);
    // Workbench and shelves, real warm emitters with dark corners between them.
    for x in [-4, -1, 3, 6] {
        s.put([x, 33, -20], world::WOOD);
    }
    s.fill([-4, 34, -20], [6, 34, -19], world::WOOD_X);
    s.fill([-4, 36, -21], [6, 36, -21], world::WOOD_X);
    for x in [-3, 4] {
        s.put([x, 37, -21], world::GLOWSTONE);
    }
    s.fill([3, 35, -20], [4, 35, -20], steel);
    s.put([-3, 35, -20], crate::content::CHEST_STATE);
    s.fill(
        [-5, 33, -16],
        [-4, 34, -14],
        crate::content::KILN_DEFAULT_STATE,
    );
    s.fill([-5, 35, -16], [-5, 44, -16], steel);
    s.put([-5, 35, -14], steel);
    s.put([6, 33, -14], crate::content::CHEST_STATE);
    s.put([6, 34, -14], crate::content::CHEST_STATE);
    // Small service marker, not a wall of neon or compensating character fill.
    s.put([5, 36, -7], steel);
    s.put([5, 37, -7], cyan);
    s.fill([9, 33, -16], [11, 34, -13], world::WOOD_Z);
    s.fill([9, 33, -10], [10, 33, -9], steel);
    // Low irregular mossy stone edges and ground cover establish foreground scale.
    for (x, z) in [
        (-8, -4),
        (-9, -3),
        (-10, -4),
        (8, 0),
        (9, -1),
        (10, -2),
        (-12, 6),
        (-13, 7),
    ] {
        s.put([x, 33, z], world::MOSS);
    }
    vegetation::plant(&mut s);
}

struct Builder<'a> {
    chunks: &'a mut HashMap<ChunkKey, Arc<world::Chunk>>,
}
impl Builder<'_> {
    fn put(&mut self, p: [i32; 3], block: world::BlockId) {
        set_preview_block(self.chunks, p[0], p[1], p[2], block);
    }
    fn fill(&mut self, a: [i32; 3], b: [i32; 3], block: world::BlockId) {
        for y in a[1]..=b[1] {
            for z in a[2]..=b[2] {
                for x in a[0]..=b[0] {
                    self.put([x, y, z], block);
                }
            }
        }
    }
    fn block(&self, p: [i32; 3]) -> Option<world::BlockId> {
        let (key, local) = world::world_to_chunk(p[0], p[1], p[2]);
        self.chunks.get(&key).and_then(|chunk| chunk.block(local))
    }
}

pub(in crate::preview) fn avatars(
    chunks: &HashMap<ChunkKey, Arc<world::Chunk>>,
) -> Vec<render::VisualAvatar> {
    let mut actors = calibration::avatars(chunks);
    actors.truncate(2);
    for (actor, p) in actors
        .iter_mut()
        .zip([[1.8, 33.0, -6.2], [-0.2, 33.0, -16.5]])
    {
        actor.position = Vec3::from_array(p);
        let p = (actor.position + Vec3::Y * 1.45).floor().as_ivec3();
        let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
        let sample = LightField::build_with_bounce(key, chunks, SEED, false).face(local, 1, 0);
        actor.light_levels = [sample.sky, sample.glow, 0, 0];
        actor.glow_color = sample.glow_color;
        actor.glow_direction = sample.glow_direction;
        actor.bounce = [0; 4];
        actor.glow_bounce = [0; 4];
        println!(
            "workshop actor {} sky={} glow={}",
            actor.id, sample.sky, sample.glow
        );
    }
    actors
}

#[cfg(test)]
mod tests;
