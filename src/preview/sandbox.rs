//! Deterministic, optional art-direction fixtures, not a gameplay world or default theme.
//! Geometry, characters, animation, camera and exposure stay fixed across lighting captures.
use super::*;
use crate::content::{BlockStateId, Catalog};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Theme {
    Workshop,
    Factory,
    Neon,
}

impl Theme {
    const ALL: [Self; 3] = [Self::Workshop, Self::Factory, Self::Neon];
    fn name(self) -> &'static str {
        match self {
            Self::Workshop => "workshop",
            Self::Factory => "factory",
            Self::Neon => "neon",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum View {
    Hero,
    Characters,
}

impl View {
    fn name(self) -> &'static str {
        match self {
            Self::Hero => "hero",
            Self::Characters => "characters",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct Shot {
    pub theme: Theme,
    pub view: View,
    pub clip: &'static str,
    pub seconds: f32,
}

mod materials;
pub use materials::install_sandbox_materials;

pub fn render_sandbox_previews(
    directory: &Path,
    theme: &str,
    time: &str,
    view: &str,
    pose: &str,
) -> Result<(), Box<dyn Error>> {
    let themes: Vec<_> = if theme == "all" {
        Theme::ALL.to_vec()
    } else {
        vec![
            Theme::ALL
                .into_iter()
                .find(|t| t.name() == theme)
                .ok_or("sandbox scene must be all, workshop, factory or neon")?,
        ]
    };
    let times = [
        ("noon", crate::daylight::INITIAL_MS),
        ("sunset", crate::daylight::CYCLE_MS * 46 / 100),
        ("night", crate::daylight::CYCLE_MS * 3 / 4),
    ];
    if time != "all" && !times.iter().any(|(name, _)| *name == time) {
        return Err("sandbox time must be all, noon, sunset or night".into());
    }
    let view = match view {
        "hero" => View::Hero,
        "characters" => View::Characters,
        _ => return Err("sandbox camera must be hero or characters".into()),
    };
    let clip = match pose {
        "idle" => "idle",
        "walk" => "walk",
        _ => return Err("sandbox pose must be idle or walk".into()),
    };
    let seconds = parse_seconds(std::env::var("BLOXGLOOM_PREVIEW_SECONDS").ok().as_deref())?;
    let sample_name = format!("{seconds}s").replace('.', "p");
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join(format!(
            "capture-settings-{}-{clip}-{sample_name}.txt",
            view.name()
        )),
        format!(
            "Sandbox fixtures v1\nProduction WGPU voxel + articulated character + HDR/postprocess\n1280x800; exposure 1.0; bloom 0.12; bounce disabled\nCamera: {}; clip: {clip}; animation sample: {seconds} seconds\nGeometry, six characters, camera and pose are identical between times.\nSix shared character recipes and material swatches match across themes.\nTextures: builtin except three startup-only sandbox materials: steel and cyan/magenta emissive panels.\nNo UI, extra character fill, auto exposure, world saves or gameplay changes.\nAdapter identity is printed to stdout. Software renders are not hardware performance evidence.\n",
            view.name()
        ),
    )?;
    for theme in themes {
        for (name, world_time) in times {
            if time != "all" && time != name {
                continue;
            }
            let name = format!(
                "sandbox-{}-{name}-{}-{clip}-{sample_name}",
                theme.name(),
                view.name()
            );
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
                PreviewScene::Sandbox(Shot {
                    theme,
                    view,
                    clip,
                    seconds,
                }),
                None,
                world_time,
            ))?;
            println!("sandbox capture: {name}");
        }
    }
    Ok(())
}

fn parse_seconds(value: Option<&str>) -> Result<f32, Box<dyn Error>> {
    let seconds: f32 = value
        .unwrap_or("0.35")
        .parse()
        .map_err(|_| "BLOXGLOOM_PREVIEW_SECONDS must be finite and in 0..=60")?;
    if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
        return Err("BLOXGLOOM_PREVIEW_SECONDS must be finite and in 0..=60".into());
    }
    Ok(seconds)
}

pub(super) fn camera(view: View) -> Camera {
    let (position, target, fov) = match view {
        View::Hero => (
            Vec3::new(19.5, 40.5, 7.5),
            Vec3::new(7.5, 35.7, -14.0),
            48.0f32,
        ),
        View::Characters => (Vec3::new(8.5, 35.5, 1.5), Vec3::new(8.5, 34.1, -10.5), 48.0),
    };
    let direction = (target - position).normalize();
    Camera {
        position,
        yaw: direction.z.atan2(direction.x),
        pitch: direction.y.asin(),
        fov_y_radians: fov.to_radians(),
    }
}

pub(super) fn prepare(shot: Shot, chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>) -> Camera {
    prepare_with_catalog(shot, chunks, crate::content::catalog())
}

fn prepare_with_catalog(
    shot: Shot,
    chunks: &mut HashMap<ChunkKey, Arc<world::Chunk>>,
    catalog: &Catalog,
) -> Camera {
    // Replace every provided chunk, including underground cells and the whole sky column.
    // Meshing/lighting cannot accidentally depend on the evolving terrain generator.
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
    let mut stage = Builder { chunks };
    stage.fill(
        [-8, 32, -29],
        [24, 32, 2],
        if shot.theme == Theme::Workshop {
            world::SAND
        } else {
            world::GRAVEL
        },
    );
    stage.fill([-1, 32, -16], [18, 32, -4], world::STONE);
    // Repeated in every theme: six stone/sand/wood/gravel/moss/cutout swatches.
    for (index, material) in [
        world::STONE,
        world::SAND,
        world::WOOD,
        world::GRAVEL,
        world::MOSS,
        world::LEAVES,
    ]
    .into_iter()
    .enumerate()
    {
        let x = 2 + index as i32 * 2;
        stage.fill([x, 33, -14], [x, 34, -14], material);
        stage.fill([x, 32, -7], [x, 32, -6], material);
    }
    match shot.theme {
        Theme::Workshop => workshop(&mut stage),
        Theme::Factory => factory(&mut stage, catalog),
        Theme::Neon => neon(&mut stage, catalog),
    }
    // Shared planted tree tests irregular opaque and alpha-cutout shadow casters.
    stage.tree(-5, -9);
    // Shared roof + deep recess creates an open sky portal and a sheltered interior.
    // The left opening faces the camera, so it can be judged without hiding actors.
    let wall = if shot.theme == Theme::Workshop {
        world::WOOD
    } else {
        world::STONE
    };
    stage.fill([-7, 33, -23], [-7, 38, -15], wall);
    stage.fill([-7, 33, -23], [-1, 38, -23], wall);
    stage.fill([-1, 33, -23], [-1, 38, -15], wall);
    stage.fill([-7, 38, -23], [-1, 38, -15], wall);
    stage.put(-4, 35, -22, world::GLOWSTONE);
    camera(shot.view)
}

fn wood_axis(axis: &str) -> BlockStateId {
    crate::content::catalog()
        .state_with_property(world::WOOD, "axis", axis)
        .expect("builtin wood axis")
}

struct Builder<'a> {
    chunks: &'a mut HashMap<ChunkKey, Arc<world::Chunk>>,
}

impl Builder<'_> {
    fn put(&mut self, x: i32, y: i32, z: i32, block: BlockStateId) {
        set_preview_block(self.chunks, x, y, z, block);
    }
    fn fill(&mut self, from: [i32; 3], to: [i32; 3], block: BlockStateId) {
        for y in from[1]..=to[1] {
            for z in from[2]..=to[2] {
                for x in from[0]..=to[0] {
                    self.put(x, y, z, block);
                }
            }
        }
    }
    fn tree(&mut self, x: i32, z: i32) {
        self.fill([x - 2, 32, z - 2], [x + 2, 32, z + 2], world::MOSS);
        self.fill([x, 33, z], [x, 38, z], world::WOOD);
        self.fill([x - 1, 37, z], [x + 2, 37, z], wood_axis("x"));
        for (y, radius) in [(37, 2i32), (38, 3), (39, 3), (40, 2), (41, 1)] {
            for dz in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() + dz.abs() <= radius + 1 {
                        self.put(x + dx, y, z + dz, world::LEAVES);
                    }
                }
            }
        }
    }
}

fn workshop(stage: &mut Builder<'_>) {
    // Timber-framed making space: warm infill, stepped pitched roof, open work bay.
    stage.fill([0, 33, -26], [18, 39, -25], world::SAND);
    stage.fill([0, 33, -25], [0, 39, -17], world::SAND);
    stage.fill([18, 33, -25], [18, 39, -17], world::SAND);
    for x in [0, 6, 12, 18] {
        stage.fill([x, 33, -17], [x, 39, -17], world::WOOD);
        stage.fill([x, 33, -25], [x, 39, -25], world::WOOD);
    }
    for z in -26i32..=-16 {
        let y = 39 + (4 - (z + 21).abs()).max(0);
        stage.fill([-1, y, z], [19, y, z], wood_axis("x"));
    }
    stage.fill([0, 37, -17], [18, 37, -17], wood_axis("x"));
    for x in [3, 9, 15] {
        stage.fill([x - 1, 35, -24], [x + 1, 36, -24], world::GLOWSTONE);
        stage.fill(
            [x - 1, 33, -20],
            [x + 1, 33, -19],
            crate::content::CHEST_STATE,
        );
        stage.fill([x - 1, 34, -20], [x + 1, 34, -19], wood_axis("x"));
    }
    stage.fill([20, 33, -21], [21, 35, -18], wood_axis("z"));
    for x in [0, 17] {
        stage.put(x, 34, -15, world::GLOWSTONE);
    }
    for (x, z) in [(-3, -5), (-6, -12), (20, -13), (22, -10)] {
        stage.put(x, 32, z, world::GRASS);
        stage.put(x, 33, z, world::FERN);
    }
}

fn factory(stage: &mut Builder<'_>, catalog: &Catalog) {
    let brick = crate::content::KILN_DEFAULT_STATE;
    let steel = catalog
        .state_by_key("sandbox:steel")
        .expect("sandbox steel registered");
    stage.fill([0, 33, -27], [19, 41, -25], brick);
    stage.fill([0, 33, -25], [0, 40, -17], brick);
    stage.fill([19, 33, -25], [19, 40, -17], brick);
    // Repeated structural steel bays and a lit clerestory under the metal roof.
    for x in [0, 6, 12, 18] {
        stage.fill([x, 33, -18], [x, 40, -18], steel);
        stage.fill([x, 41, -25], [x + 1, 41, -17], steel);
    }
    stage.fill([0, 40, -18], [19, 40, -18], steel);
    for x in 1..19 {
        if x % 3 != 0 {
            stage.put(x, 38, -24, world::GLOWSTONE);
        }
    }
    // Twin boiler stacks and overhead service pipe; entirely static fixture geometry.
    for x in [3, 15] {
        stage.fill([x, 33, -23], [x + 2, 36, -20], brick);
        stage.fill([x + 1, 37, -22], [x + 1, 45, -22], steel);
        stage.put(x + 1, 34, -19, world::GLOWSTONE);
    }
    stage.fill([1, 37, -20], [17, 37, -20], steel);
    stage.fill([21, 33, -24], [22, 43, -23], brick);
    for x in [0, 18] {
        stage.put(x, 34, -15, world::GLOWSTONE);
    }
    stage.fill([20, 33, -16], [22, 34, -14], crate::content::CHEST_STATE);
    // Inlaid tracks guide the eye to the production floor without burying feet.
    for x in [0, 17] {
        stage.fill([x, 32, -24], [x, 32, -4], steel);
    }
}

fn neon(stage: &mut Builder<'_>, catalog: &Catalog) {
    let cyan = catalog
        .state_by_key("sandbox:cyan")
        .expect("sandbox cyan registered");
    let magenta = catalog
        .state_by_key("sandbox:magenta")
        .expect("sandbox magenta registered");
    let steel = catalog
        .state_by_key("sandbox:steel")
        .expect("sandbox steel registered");
    // Modular industrial hangar. The fixtures are real emissive voxel surfaces,
    // not screen-space overlays or a separate reference-rendering pipeline.
    stage.fill([0, 33, -27], [19, 41, -25], steel);
    stage.fill([0, 33, -25], [0, 40, -17], world::STONE);
    stage.fill([19, 33, -25], [19, 40, -17], world::STONE);
    for x in [0, 6, 12, 18] {
        stage.fill([x, 33, -18], [x, 40, -18], steel);
        stage.fill([x, 41, -25], [x, 41, -18], steel);
        stage.put(x, 39, -17, cyan);
    }
    stage.fill([0, 40, -18], [19, 40, -18], steel);
    stage.fill([1, 39, -24], [18, 39, -24], cyan);
    for x in [2, 8, 14] {
        stage.fill([x, 33, -23], [x + 2, 36, -21], world::STONE);
        stage.fill([x, 35, -20], [x + 2, 35, -20], magenta);
        stage.put(x + 1, 37, -22, cyan);
    }
    stage.fill([20, 33, -24], [22, 43, -23], steel);
    stage.fill([21, 35, -22], [21, 41, -22], magenta);
    for x in [0, 18] {
        stage.fill([x, 32, -16], [x, 32, -4], cyan);
        stage.put(x, 34, -15, magenta);
    }
    stage.fill([20, 33, -16], [22, 34, -14], steel);
}

pub(super) fn avatars(chunks: &HashMap<ChunkKey, Arc<world::Chunk>>) -> Vec<render::VisualAvatar> {
    let mut actors = calibration::avatars(chunks);
    let mut fields = HashMap::new();
    for actor in &mut actors {
        actor.position.z = -10.0;
        let p = (actor.position + Vec3::Y * 1.45).floor().as_ivec3();
        let (key, local) = world::world_to_chunk(p.x, p.y, p.z);
        let field = fields
            .entry(key)
            .or_insert_with(|| LightField::build_with_bounce(key, chunks, SEED, false));
        let sample = field.face(local, 1, 0);
        actor.light_levels = [sample.sky, sample.glow, 0, 0];
        actor.bounce = [sample.bounce[0], sample.bounce[1], sample.bounce[2], 0];
        actor.glow_bounce = [
            sample.glow_bounce[0],
            sample.glow_bounce[1],
            sample.glow_bounce[2],
            0,
        ];
    }
    actors
}

#[cfg(test)]
#[path = "sandbox/tests.rs"]
mod tests;
